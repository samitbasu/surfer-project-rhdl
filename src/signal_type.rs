#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SignalType {
    Logic,
    Integer,
    Real,
    Parameter,
    String,
    Time,
    Struct,
    // VCD-specific types
    VCDEvent,
    VCDRealTime,
    VCDSupply0,
    VCDSupply1,
    VCDTri,
    VCDTriAnd,
    VCDTriOr,
    VCDTriReg,
    VCDTri0,
    VCDTri1,
    VCDWAnd,
    VCDWOr,
}

impl From<fastwave_backend::SignalType> for SignalType {
    fn from(signaltype: fastwave_backend::SignalType) -> Self {
        match signaltype {
            fastwave_backend::SignalType::Reg => SignalType::Logic,
            fastwave_backend::SignalType::Wire => SignalType::Logic,
            fastwave_backend::SignalType::Integer => SignalType::Integer,
            fastwave_backend::SignalType::Real => SignalType::Real,
            fastwave_backend::SignalType::Parameter => SignalType::Parameter,
            fastwave_backend::SignalType::Str => SignalType::String,
            fastwave_backend::SignalType::Time => SignalType::Time,
            fastwave_backend::SignalType::Event => SignalType::VCDEvent,
            fastwave_backend::SignalType::RealTime => SignalType::VCDRealTime,
            fastwave_backend::SignalType::Supply0 => SignalType::VCDSupply0,
            fastwave_backend::SignalType::Supply1 => SignalType::VCDSupply1,
            fastwave_backend::SignalType::Tri => SignalType::VCDTri,
            fastwave_backend::SignalType::TriAnd => SignalType::VCDTriAnd,
            fastwave_backend::SignalType::TriOr => SignalType::VCDTriOr,
            fastwave_backend::SignalType::TriReg => SignalType::VCDTriReg,
            fastwave_backend::SignalType::Tri0 => SignalType::VCDTri0,
            fastwave_backend::SignalType::Tri1 => SignalType::VCDTri1,
            fastwave_backend::SignalType::WAnd => SignalType::VCDWAnd,
            fastwave_backend::SignalType::WOr => SignalType::VCDWOr,
        }
    }
}
