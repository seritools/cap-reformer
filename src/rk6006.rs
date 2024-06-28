use snafu::Snafu;
use std::time::Duration;
use tokio_modbus::{
    client::{Context, Reader, Writer},
    Slave, SlaveId,
};
use tokio_serial::{DataBits, SerialPortBuilderExt, StopBits};

const SERIAL_TIMEOUT: Duration = Duration::from_millis(500);
const VOLT_DIVIDER: f64 = 100.0;
const CURRENT_DIVIDER: f64 = 1000.0;

#[derive(Debug, Snafu)]
pub enum PsuModbusError {
    #[snafu(context(false))]
    SerialOpen { source: tokio_serial::Error },
    #[snafu(context(false))]
    ModbusProtocol { source: tokio_modbus::Error },
    #[snafu(context(false))]
    ModbusException { source: tokio_modbus::Exception },
}

pub async fn open_psu_modbus(
    serial_path: String,
    slave_id: SlaveId,
) -> Result<Psu, PsuModbusError> {
    let serial_stream = tokio_serial::new(serial_path, 115200)
        .data_bits(DataBits::Eight)
        .stop_bits(StopBits::One)
        .timeout(SERIAL_TIMEOUT)
        .open_native_async()?;

    let mut psu = tokio_modbus::client::rtu::attach_slave(serial_stream, Slave(slave_id));

    let regs = psu.read_holding_registers(0, 4).await??;
    let sn: u32 = (regs[1] as u32) << 16 | (regs[2] as u32);
    println!(
        "Type: {}, FW: {}, SN: {sn:08X}",
        regs[0] / 10,
        regs[3] as f64 / 100.0
    );

    Ok(Psu { ctx: psu })
}

pub struct Psu {
    ctx: Context,
}

impl Psu {
    pub async fn disconnect(&mut self) -> Result<(), PsuModbusError> {
        Ok(self.ctx.disconnect().await??)
    }

    pub async fn set_output(&mut self, enable: bool) -> Result<(), PsuModbusError> {
        Ok(self.ctx.write_single_register(18, enable as u16).await??)
    }

    pub async fn set_voltage(&mut self, voltage: f64) -> Result<(), PsuModbusError> {
        Ok(self
            .ctx
            .write_single_register(8, (voltage * VOLT_DIVIDER) as u16)
            .await??)
    }
    pub async fn set_voltage_protection(&mut self, voltage: f64) -> Result<(), PsuModbusError> {
        Ok(self
            .ctx
            .write_single_register(82, (voltage * VOLT_DIVIDER) as u16)
            .await??)
    }
    pub async fn set_current(&mut self, current: f64) -> Result<(), PsuModbusError> {
        Ok(self
            .ctx
            .write_single_register(9, (current * CURRENT_DIVIDER) as u16)
            .await??)
    }
    pub async fn set_current_protection(&mut self, current: f64) -> Result<(), PsuModbusError> {
        Ok(self
            .ctx
            .write_single_register(83, (current * CURRENT_DIVIDER) as u16)
            .await??)
    }

    pub async fn voltage_and_current(&mut self) -> Result<(f64, f64), PsuModbusError> {
        let regs = self.ctx.read_holding_registers(10, 2).await??;
        Ok((
            regs[0] as f64 / VOLT_DIVIDER,
            regs[1] as f64 / CURRENT_DIVIDER,
        ))
    }

    pub async fn voltage(&mut self) -> Result<f64, PsuModbusError> {
        let regs = self.ctx.read_holding_registers(10, 1).await??;
        Ok(regs[0] as f64 / VOLT_DIVIDER)
    }
}
