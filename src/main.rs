mod owon;
mod rk6006;

use argh::FromArgs;
use core::panic;
use owon::mode::Mode;
use rk6006::{Psu, PsuModbusError};
use snafu::{ensure, OptionExt, Snafu};
use std::{
    error::Error,
    fmt::Debug,
    time::{Duration, Instant},
};
use tokio::{sync::watch, task::JoinHandle};
use tokio_modbus::SlaveId;
use tokio_util::sync::CancellationToken;

#[derive(Debug, FromArgs)]
/// Capacitor reformer
struct Config {
    /// serial port to use
    #[argh(positional)]
    serial_port: String,

    /// rated voltage of the capacitor in V
    #[argh(positional)]
    voltage: f64,

    /// rated capacitance of the capacitor in µF (optional). This is only used for CV display purposes.
    #[argh(positional)]
    capacitance: Option<f64>,

    /// sets the Modbus slave ID. Default: 1
    #[argh(option, default = "1")]
    slave_id: SlaveId,

    /// reform current. increases voltage by `voltage_step` if current falls below this value and
    /// rated voltage has not been reached. Default: 2.5mA
    #[argh(option, default = "2.5")]
    reform_current: f64,

    /// current in mA to drop to when the capacitor is at the rated voltage, to finish
    /// reforming. Default: 0.02mA
    #[argh(option, default = "0.02")]
    finish_current: f64,

    /// increase by this voltage if reform current is not reached. Default: 0.5V
    #[argh(option, default = "0.5")]
    voltage_step: f64,

    /// immediately cut power and abort reforming if current goes above this value. Default: 10mA
    #[argh(option, default = "10.0")]
    current_limit: f64,

    /// current at which the power supply should go into constant current mode and drop voltage, in milliamps. Default: 30mA
    #[argh(option, default = "30.0")]
    psu_current_limit: f64,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let config: Config = argh::from_env();

    let cancel = CancellationToken::new();
    let reform_task_cancel_token = cancel.clone();

    let ctrlc_cancel = cancel.clone();
    ctrlc::set_handler(move || {
        println!("Ctrl-C received, cancelling...");
        ctrlc_cancel.cancel();
    })?;

    println!("Starting reforming with config:\n{:#?}", config);

    println!("Connecting to PSU...");
    let mut psu = rk6006::open_psu_modbus(config.serial_port.clone(), config.slave_id).await?;
    let (bt_tx, bt_rx) = watch::channel(None);

    println!("Connecting to Multimeter...");
    let mut bt_task = owon::start_bt_message_stream_task(cancel.clone(), bt_tx).await?;

    let mut logic_task: JoinHandle<Result<(), Box<dyn Error + Send + Sync>>> =
        tokio::spawn(async move {
            let res = reform_cap(&mut psu, reform_task_cancel_token, bt_rx, &config).await;
            psu.set_output(false).await?;
            let _ = psu.disconnect().await;
            res?;

            Ok(())
        });

    tokio::select! {
        res = &mut bt_task => {
            match res {
                Ok(Err(e)) => {
                    eprintln!("Error in BT task: {:#?}", e);
                }
                Err(e) => {
                    eprintln!("Join error in BT task: {:#?}", e);
                }
                _ => {}
            }
            cancel.cancel();
            let _ = logic_task.await;
        }
        res = &mut logic_task => {
            match res {
                Ok(Err(e)) => {
                    eprintln!("Error in logic task: {:#?}", e);
                }
                Err(e) => {
                    eprintln!("Join error in logic task: {:#?}", e);
                }
                _ => {}
            }
            cancel.cancel();
            let _ = bt_task.await;
        }
    }

    Ok(())
}

#[derive(Debug, Snafu)]
enum ReformCapError {
    #[snafu(context(false))]
    PsuModbus { source: PsuModbusError },
    #[snafu(context(false))]
    BtChannelClosed { source: watch::error::RecvError },
    /// No reading available from the multimeter
    NoBtReading,
    #[snafu(display("Wrong reading mode, got {mode:#?}"))]
    WrongReadingMode { mode: owon::mode::Mode },

    /// Aborted reforming because the current limit was exceeded
    CapCurrentLimitExceeded,
}

async fn reform_cap(
    psu: &mut Psu,
    cancel: CancellationToken,
    mut reading_rx: watch::Receiver<Option<owon::reading::Reading>>,
    config: &Config,
) -> Result<(), ReformCapError> {
    let Config {
        serial_port: _,
        slave_id: _,
        capacitance,
        voltage: rated_voltage,
        reform_current,
        finish_current,
        voltage_step,
        current_limit,
        psu_current_limit,
    } = *config;

    let reform_current_milliamps = reform_current;
    let finish_current_milliamps = finish_current;
    let current_limit_milliamps = current_limit;

    let mut last_voltage_increase = Instant::now();

    let mut curr_voltage = 0.0;
    psu.set_voltage(curr_voltage).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    psu.set_current(psu_current_limit / 1000.0).await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    psu.set_output(true).await?;

    println!("Reforming...");
    loop {
        if cancel.is_cancelled() {
            return Ok(());
        }
        let milliamps = current_milliamps(&mut reading_rx).await?;
        print_measurement(rated_voltage, capacitance, curr_voltage, milliamps);

        ensure!(
            milliamps < current_limit_milliamps,
            CapCurrentLimitExceededSnafu
        );

        if milliamps < reform_current_milliamps
            && last_voltage_increase.elapsed() > Duration::from_secs(1)
        {
            if curr_voltage == rated_voltage {
                break;
            }

            curr_voltage = (curr_voltage + voltage_step).min(rated_voltage);
            psu.set_voltage(curr_voltage).await?;
            last_voltage_increase = Instant::now();
        }
    }

    println!(
        "Target voltage reached, waiting to reach target current (< {:.3}mA)...",
        finish_current_milliamps
    );

    loop {
        if cancel.is_cancelled() {
            break;
        }
        let milliamps = current_milliamps(&mut reading_rx).await?;
        print_measurement(rated_voltage, capacitance, curr_voltage, milliamps);

        ensure!(
            milliamps < current_limit_milliamps,
            CapCurrentLimitExceededSnafu
        );

        if milliamps < finish_current_milliamps {
            println!("Reforming complete");
            break;
        }
    }

    Ok(())
}

async fn current_milliamps(
    reading_rx: &mut watch::Receiver<Option<owon::reading::Reading>>,
) -> Result<f64, ReformCapError> {
    reading_rx.changed().await?;
    let reading = reading_rx
        .borrow_and_update()
        .as_ref()
        .copied()
        .context(NoBtReadingSnafu)?;

    let multimeter_milliamps = match reading.mode {
        Mode::DcMilliAmpere => reading.value(),
        mode => return Err(ReformCapError::WrongReadingMode { mode }),
    };

    Ok(multimeter_milliamps)
}

fn print_measurement(rated_voltage: f64, capacitance: Option<f64>, voltage: f64, milliamps: f64) {
    if let Some(capacitance) = capacitance {
        println!(
            "Reform current: {milliamps:.3}mA ({:.5} CV) at {voltage:.2}V",
            milliamps * 1000.0 / (rated_voltage * capacitance),
        );
    } else {
        println!("Reform current: {milliamps:.3}mA at {voltage:.2}V",);
    }
}
