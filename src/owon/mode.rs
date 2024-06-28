use bytemuck::CheckedBitPattern;

#[repr(u16)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, CheckedBitPattern)]
#[allow(dead_code)]
pub enum Mode {
    DcMillivolt = 0xF018,
    DcVolt = 0xF020,
    AcMillivolt = 0xF058,
    AcVolt = 0xF060,
    DcMicroAmpere = 0xF090,
    DcMilliAmpere = 0xF098,
    DcAmpere = 0xF0A0,
    AcMicroAmpere = 0xF0D0,
    AcMilliAmpere = 0xF0D8,
    AcAmpere = 0xF0E0,
    Ohm = 0xF120,
    KiloOhm = 0xF128,
    MegaOhm = 0xF130,
    NanoFarad = 0xF148,
    MicroFarad = 0xF150,
    MilliFarad = 0xF158,
    Farad = 0xF160,
    Hertz = 0xF1A0,
    KiloHertz = 0xF1A8,
    MegaHertz = 0xF1B0,
    DutyCyclePercent = 0xF1E0,
    DegreesCelsius = 0xF220,
    DegreesFahrenheit = 0xF260,
    DiodeVolt = 0xF2A0,
    ContinuityOhm = 0xF2E0,
    NearField = 0xF360,
}

impl Mode {
    pub fn as_str(&self) -> &'static str {
        match self {
            Mode::DcMillivolt => "mV DC",
            Mode::DcVolt => "V DC",
            Mode::AcMillivolt => "mV AC",
            Mode::AcVolt => "V AC",
            Mode::DcMicroAmpere => "µA DC",
            Mode::DcMilliAmpere => "mA DC",
            Mode::DcAmpere => "A DC",
            Mode::AcMicroAmpere => "µA AC",
            Mode::AcMilliAmpere => "mA AC",
            Mode::AcAmpere => "A AC",
            Mode::Ohm => "Ω",
            Mode::KiloOhm => "kΩ",
            Mode::MegaOhm => "MΩ",
            Mode::NanoFarad => "nF",
            Mode::MicroFarad => "µF",
            Mode::MilliFarad => "mF",
            Mode::Farad => "F",
            Mode::Hertz => "Hz",
            Mode::KiloHertz => "kHz",
            Mode::MegaHertz => "MHz",
            Mode::DutyCyclePercent => "%",
            Mode::DegreesCelsius => "°C",
            Mode::DegreesFahrenheit => "°F",
            Mode::DiodeVolt => "V ―⯈⊢",
            Mode::ContinuityOhm => "Ohm ))))",
            Mode::NearField => "(NF)",
        }
    }
}
