use surfer_translation_types::VariableType;
use wellen::VarType;

#[local_impl::local_impl]
impl VariableTypeExt for VariableType {
    fn from_wellen_type(var_type: VarType) -> VariableType {
        match var_type {
            VarType::Reg => VariableType::VCDReg,
            VarType::Wire => VariableType::VCDWire,
            VarType::Integer => VariableType::VCDInteger,
            VarType::Real => VariableType::VCDReal,
            VarType::Parameter => VariableType::VCDParameter,
            VarType::String => VariableType::VCDString,
            VarType::Time => VariableType::VCDTime,
            VarType::Event => VariableType::VCDEvent,
            VarType::Supply0 => VariableType::VCDSupply0,
            VarType::Supply1 => VariableType::VCDSupply1,
            VarType::Tri => VariableType::VCDTri,
            VarType::TriAnd => VariableType::VCDTriAnd,
            VarType::TriOr => VariableType::VCDTriOr,
            VarType::TriReg => VariableType::VCDTriReg,
            VarType::Tri0 => VariableType::VCDTri0,
            VarType::Tri1 => VariableType::VCDTri1,
            VarType::WAnd => VariableType::VCDWAnd,
            VarType::WOr => VariableType::VCDWOr,
            VarType::Port => VariableType::Port,
            VarType::Bit => VariableType::Bit,
            VarType::Logic => VariableType::Logic,
            VarType::Int => VariableType::VCDInteger,
            VarType::Enum => VariableType::Enum,
            VarType::SparseArray => VariableType::SparseArray,
            VarType::RealTime => VariableType::RealTime,
            VarType::ShortInt => VariableType::ShortInt,
            VarType::LongInt => VariableType::LongInt,
            VarType::Byte => VariableType::Byte,
            VarType::ShortReal => VariableType::ShortReal,
            VarType::Boolean => VariableType::Boolean,
            VarType::BitVector => VariableType::BitVector,
            VarType::StdLogic => VariableType::StdLogic,
            VarType::StdLogicVector => VariableType::StdLogicVector,
            VarType::StdULogic => VariableType::StdULogic,
            VarType::StdULogicVector => VariableType::StdULogicVector,
        }
    }
}

/// Types that should default to signed conversion
pub const INTEGER_TYPES: &[Option<VariableType>] = &[
    Some(VariableType::VCDInteger),
    Some(VariableType::Int),
    Some(VariableType::ShortInt),
    Some(VariableType::LongInt),
];
