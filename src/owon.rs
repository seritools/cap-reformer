pub mod mode;
pub mod reading;

use btleplug::{
    api::{Central, CentralEvent, Manager as _, Peripheral, ScanFilter, ValueNotification},
    platform::Manager,
};
use futures_lite::{Stream, StreamExt};
use mode::Mode;
use snafu::{ensure, Snafu};
use std::{ops::ControlFlow, time::Duration};
use tokio::{sync::watch, task::JoinHandle, time};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

const OW18E_SERVICE: Uuid = uuid::uuid!("0000fff0-0000-1000-8000-00805f9b34fb");
// const OW18E_WRITE_CHARACTERISTIC: Uuid = uuid::uuid!("0000fff3-0000-1000-8000-00805f9b34fb");
const OW18E_NOTIFY_CHARACTERISTIC: Uuid = uuid::uuid!("0000fff4-0000-1000-8000-00805f9b34fb");

const TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Snafu)]
pub enum StartBtMessageStreamError {
    #[snafu(context(false))]
    Btle {
        source: btleplug::Error,
    },
    InitialNotificationDidNotArrive,
    /// The multimeter is in the wrong mode. It must be set to DC milliampere mode.
    MultimeterInWrongMode,
}

pub async fn start_bt_message_stream_task(
    cancel: CancellationToken,
    reading_tx: watch::Sender<Option<reading::Reading>>,
) -> Result<JoinHandle<Result<(), btleplug::Error>>, StartBtMessageStreamError> {
    let manager = Manager::new().await?;
    let adapter_list = manager.adapters().await?;
    let Some(adapter) = adapter_list.into_iter().next() else {
        panic!("No Bluetooth adapter found");
    };

    println!(
        "Using adapter {}, searching for OW18E_SERVICE device...",
        adapter.adapter_info().await?
    );

    let mut events = adapter.events().await?;
    adapter
        .start_scan(ScanFilter {
            services: vec![OW18E_SERVICE],
        })
        .await?;

    let device_id = loop {
        let Some(next) = events.next().await else {
            panic!("Event stream ended without finding any devices");
        };

        if let CentralEvent::DeviceDiscovered(id) = next {
            println!("Found device ({id:?})");
            break id;
        }
    };

    drop(events);
    let device = adapter.peripheral(&device_id).await?;

    let properties = device.properties().await?;
    let is_connected = device.is_connected().await?;
    let local_name = properties
        .unwrap()
        .local_name
        .unwrap_or(String::from("[unnamed]"));

    if is_connected {
        println!("Device {local_name} is already connected");
    } else {
        println!("Connecting to device {local_name}...");
        device.connect().await?;
    }

    let is_connected = device.is_connected().await?;
    assert!(is_connected);

    device.discover_services().await?;
    let service = device
        .services()
        .into_iter()
        .find(|svc| svc.uuid == OW18E_SERVICE)
        .unwrap();

    let notify_characteristic = service
        .characteristics
        .iter()
        .find(|c| c.uuid == OW18E_NOTIFY_CHARACTERISTIC)
        .expect("Could not find notify characteristic");

    let mut notifications = device
        .notifications()
        .await?
        .filter(|n| n.uuid == OW18E_NOTIFY_CHARACTERISTIC);

    device.subscribe(notify_characteristic).await?;

    println!("Waiting for initial reading...");
    let ControlFlow::Continue(initial_reading) =
        read_notification(&cancel, &mut notifications).await?
    else {
        return InitialNotificationDidNotArriveSnafu.fail();
    };

    ensure!(
        initial_reading.mode == Mode::DcMilliAmpere,
        MultimeterInWrongModeSnafu
    );

    println!("Initial reading valid, starting message stream.");
    let bt_task: JoinHandle<Result<(), btleplug::Error>> = tokio::spawn(async move {
        loop {
            let flow = read_notification(&cancel, &mut notifications).await?;
            match flow {
                ControlFlow::Continue(reading) => {
                    if reading_tx.send(Some(reading)).is_err() {
                        break;
                    }
                }
                ControlFlow::Break(()) => break,
            }
        }

        let is_connected = device.is_connected().await?;
        if is_connected {
            println!("Disconnecting from peripheral {device_id}...");
            device
                .disconnect()
                .await
                .expect("Error disconnecting from BLE peripheral");
        }

        Ok(())
    });

    Ok(bt_task)
}

async fn read_notification(
    cancel: &CancellationToken,
    notifications: &mut (impl Stream<Item = ValueNotification> + Unpin),
) -> Result<ControlFlow<(), reading::Reading>, btleplug::Error> {
    tokio::select! {
        _ = cancel.cancelled() => Ok(ControlFlow::Break(())),
        _ = time::sleep(TIMEOUT) => {
            eprintln!("Timeout waiting for notification");
            Ok(ControlFlow::Break(()))
        }
        message = notifications.next() => {
            match message {
                Some(value) => match reading::parse(&value.value) {
                    Some(reading) => Ok(ControlFlow::Continue(reading)),
                    None => {
                        eprintln!("Invalid message received. Raw: {:#?}", value.value);
                        Ok(ControlFlow::Break(()))
                    }
                },
                None => Ok(ControlFlow::Break(())),
            }
        }
    }
}
