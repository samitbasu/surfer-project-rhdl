use std::fmt;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum VariableType {
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
    Port,
    SparseArray,
    RealTime,
    // System Verilog
    Bit,
    Logic,
    Int,
    ShortInt,
    LongInt,
    Byte,
    Enum,
    ShortReal,
    // VHDL (these are the types emitted by GHDL)
    Boolean,
    BitVector,
    StdLogic,
    StdLogicVector,
    StdULogic,
    StdULogicVector,
}

impl fmt::Display for VariableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VariableType::VCDReg => write!(f, "reg"),
            VariableType::VCDWire => write!(f, "wire"),
            VariableType::VCDInteger => write!(f, "integer"),
            VariableType::VCDReal => write!(f, "real"),
            VariableType::VCDParameter => write!(f, "parameter"),
            VariableType::VCDString => write!(f, "string"),
            VariableType::VCDEvent => write!(f, "event"),
            VariableType::VCDTime => write!(f, "time"),
            VariableType::VCDRealTime => write!(f, "real time"),
            VariableType::VCDSupply0 => write!(f, "supply 0"),
            VariableType::VCDSupply1 => write!(f, "supply 1"),
            VariableType::VCDTri => write!(f, "tri"),
            VariableType::VCDTri0 => write!(f, "tri 0"),
            VariableType::VCDTri1 => write!(f, "tri 1"),
            VariableType::VCDTriAnd => write!(f, "tri and"),
            VariableType::VCDTriOr => write!(f, "tri or"),
            VariableType::VCDTriReg => write!(f, "tri reg"),
            VariableType::VCDWAnd => write!(f, "wand"),
            VariableType::VCDWOr => write!(f, "wor"),
            VariableType::Port => write!(f, "port"),
            VariableType::Bit => write!(f, "bit"),
            VariableType::Logic => write!(f, "logic"),
            VariableType::Enum => write!(f, "enum"),
            VariableType::SparseArray => write!(f, "sparse array"),
            VariableType::RealTime => write!(f, "realtime"),
            VariableType::Int => write!(f, "int"),
            VariableType::ShortInt => write!(f, "shortint"),
            VariableType::LongInt => write!(f, "longint"),
            VariableType::Byte => write!(f, "byte"),
            VariableType::ShortReal => write!(f, "shortreal"),
            VariableType::Boolean => write!(f, "boolean"),
            VariableType::BitVector => write!(f, "bit_vector"),
            VariableType::StdLogic => write!(f, "std_logic"),
            VariableType::StdLogicVector => write!(f, "std_logic_vector"),
            VariableType::StdULogic => write!(f, "std_ulogic"),
            VariableType::StdULogicVector => write!(f, "std_ulogic_vector"),
        }
    }
}

/// Types that should default to signed conversion
pub const INTEGER_TYPES: &[Option<VariableType>] = &[Some(VariableType::VCDInteger)];

/// Types that are strings, so no conversion should be made
pub const STRING_TYPES: &[Option<VariableType>] = &[
    Some(VariableType::VCDString),
    Some(VariableType::VCDReal),
    Some(VariableType::VCDRealTime),
];
