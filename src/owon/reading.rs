use super::mode::Mode;
use binrw::BinRead;
use std::{fmt::Debug, io::Cursor};

#[derive(Clone, Copy, PartialEq)]
pub struct Reading {
    pub mode: Mode,
    divider: u8,
    raw_value: u16,
    hold: bool,
    relative: bool,
    autoranging: bool,
    low_battery: bool,
}

impl Reading {
    pub fn new(raw: RawMessage) -> Option<Self> {
        let mode = bytemuck::checked::try_pod_read_unaligned(&raw.mode().to_ne_bytes()).ok()?;
        let divider = raw.divider();
        let raw_value = raw.raw_value;
        let hold = raw.flags & 0x1 != 0;
        let relative = raw.flags & 0x2 != 0;
        let autoranging = raw.flags & 0x4 != 0;
        let low_battery = raw.flags & 0x8 != 0;

        Some(Self {
            mode,
            divider,
            raw_value,
            hold,
            relative,
            autoranging,
            low_battery,
        })
    }

    pub fn value(&self) -> f64 {
        let num = self.raw_value & 0x7FFF;
        if num == 0x7FFF {
            return f64::NAN;
        }

        let mut num = num as f64;
        if self.raw_value & 0x8000 != 0 {
            num = -num;
        }

        let divider = 10.0_f64.powi(self.divider as i32);
        num / divider
    }
}

impl Debug for Reading {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut f = f.debug_struct("Message");
        f.field("raw", &self.raw_value);

        let value = self.value();
        if value.is_nan() {
            f.field("value", &format_args!("-- {}", self.mode.as_str()));
        } else {
            f.field(
                "value",
                &format_args!(
                    "{:.*} {}",
                    usize::from(self.divider),
                    value,
                    self.mode.as_str(),
                ),
            );
        };

        f.field("hold", &self.hold)
            .field("relative", &self.relative)
            .field("autoranging", &self.autoranging)
            .field("low_battery", &self.low_battery)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, BinRead)]
#[br(little)]
pub struct RawMessage {
    mode_and_divider: u16,
    flags: u8,
    _unknown: u8,
    raw_value: u16,
}

impl RawMessage {
    pub fn mode(&self) -> u16 {
        self.mode_and_divider & !0x7
    }

    pub fn divider(&self) -> u8 {
        (self.mode_and_divider & 0x7) as u8
    }
}

pub fn parse(message: &[u8]) -> Option<Reading> {
    let raw = RawMessage::read(&mut Cursor::new(message)).ok()?;
    Reading::new(raw)
}
