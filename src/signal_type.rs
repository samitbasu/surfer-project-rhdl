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
    // FST Type
    Port,
    Bit,
    Logic,
    Enum,
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
            SignalType::Port => write!(f, "port"),
            SignalType::Bit => write!(f, "bit"),
            SignalType::Logic => write!(f, "logic"),
            SignalType::Enum => write!(f, "enum"),
        }
    }
}

/// Types that should default to signed conversion
pub const INTEGER_TYPES: &[Option<SignalType>] = &[Some(SignalType::VCDInteger)];

/// Types that are strings, so no conversion should be made
pub const STRING_TYPES: &[Option<SignalType>] = &[
    Some(SignalType::VCDString),
    Some(SignalType::VCDReal),
    Some(SignalType::VCDRealTime),
];
