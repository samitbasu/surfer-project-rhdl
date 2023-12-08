use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum SignalType {
    // VCD-specific types
    VCDEvent,
    VCDReg,
    VCDWire,
    VCDReal,
    VCDTime,
    VCDString,
    VCDParameter,
    VCDInteger,
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
            fastwave_backend::SignalType::Reg => SignalType::VCDReg,
            fastwave_backend::SignalType::Wire => SignalType::VCDWire,
            fastwave_backend::SignalType::Integer => SignalType::VCDInteger,
            fastwave_backend::SignalType::Real => SignalType::VCDReal,
            fastwave_backend::SignalType::Parameter => SignalType::VCDParameter,
            fastwave_backend::SignalType::Str => SignalType::VCDString,
            fastwave_backend::SignalType::Time => SignalType::VCDTime,
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

impl fmt::Display for SignalType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalType::VCDReg => write!(f, "reg"),
            SignalType::VCDWire => write!(f, "wire"),
            SignalType::VCDInteger => write!(f, "integer"),
            SignalType::VCDReal => write!(f, "real"),
            SignalType::VCDParameter => write!(f, "parameter"),
            SignalType::VCDString => write!(f, "string"),
            SignalType::VCDEvent => write!(f, "event"),
            SignalType::VCDTime => write!(f, "time"),
            SignalType::VCDRealTime => write!(f, "real time"),
            SignalType::VCDSupply0 => write!(f, "supply 0"),
            SignalType::VCDSupply1 => write!(f, "supply 1"),
            SignalType::VCDTri => write!(f, "tri"),
            SignalType::VCDTri0 => write!(f, "tri 0"),
            SignalType::VCDTri1 => write!(f, "tri 1"),
            SignalType::VCDTriAnd => write!(f, "tri and"),
            SignalType::VCDTriOr => write!(f, "tri or"),
            SignalType::VCDTriReg => write!(f, "tri reg"),
            SignalType::VCDWAnd => write!(f, "wand"),
            SignalType::VCDWOr => write!(f, "wor"),
        }
    }
}
