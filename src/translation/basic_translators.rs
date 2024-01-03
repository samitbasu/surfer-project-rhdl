use super::{
    check_single_wordlength, BasicTranslator, SignalInfo, TranslationPreference, ValueKind,
};
use crate::wave_container::{SignalMeta, SignalValue};

use color_eyre::Result;
use itertools::Itertools;
use num::Zero;

// Forms groups of n chars from from a string. If the string size is
// not divisible by n, the first group will be smaller than n
// The string must only consist of ascii characters
pub fn group_n_chars(s: &str, n: usize) -> Vec<&str> {
    let num_extra_chars = s.len() % n;

    let last_group = &s[0..num_extra_chars];

    let rest_groups = s.len() / n;
    let rest_str = &s[num_extra_chars..];

    if !last_group.is_empty() {
        vec![last_group]
    } else {
        vec![]
    }
    .into_iter()
    .chain((0..rest_groups).map(|start| &rest_str[start * n..(start + 1) * n]))
    .collect()
}

/// Number of digits for digit_size, simply ceil(num_bits/digit_size)
fn no_of_digits(num_bits: u64, digit_size: u64) -> usize {
    if (num_bits % digit_size) == 0 {
        (num_bits / digit_size) as usize
    } else {
        (num_bits / digit_size + 1) as usize
    }
}

/// VCD bit extension
fn extend_string(val: &String, num_bits: u64) -> String {
    if num_bits > val.len() as u64 {
        let extra_count = num_bits - val.len() as u64;
        let extra_value = match val.chars().next() {
            Some('0') => "0",
            Some('1') => "0",
            Some('x') => "x",
            Some('z') => "z",
            // If we got weird characters, this is probably a string, so we don't
            // do the extension
            // We may have to add extensions for std_logic values though if simulators save without extension
            _ => "",
        };
        extra_value.repeat(extra_count as usize)
    } else {
        String::new()
    }
}

/// Turn vector signal string into name and corresponding color if it
/// includes values other than 0 and 1. If only 0 and 1, return None.
fn check_vector_signal(s: &str) -> Option<(String, ValueKind)> {
    if s.contains('x') {
        Some(("UNDEF".to_string(), ValueKind::Undef))
    } else if s.contains('z') {
        Some(("HIGHIMP".to_string(), ValueKind::HighImp))
    } else if s.contains('-') {
        Some(("DON'T CARE".to_string(), ValueKind::DontCare))
    } else if s.contains('u') {
        Some(("UNDEF".to_string(), ValueKind::Undef))
    } else if s.contains('w') {
        Some(("UNDEF WEAK".to_string(), ValueKind::Undef))
    } else if s.contains('h') || s.contains('l') {
        Some(("WEAK".to_string(), ValueKind::Weak))
    } else if s.chars().all(|c| c == '0' || c == '1') {
        None
    } else {
        Some(("UNKNOWN VALUES".to_string(), ValueKind::Undef))
    }
}

/// Return kind for a binary representation
fn color_for_binary_representation(s: &str) -> ValueKind {
    if s.contains('x') {
        ValueKind::Undef
    } else if s.contains('z') {
        ValueKind::HighImp
    } else if s.contains('-') {
        ValueKind::DontCare
    } else if s.contains('u') || s.contains('w') {
        ValueKind::Undef
    } else if s.contains('h') || s.contains('l') {
        ValueKind::Weak
    } else {
        ValueKind::Normal
    }
}

/// Map to radix-based representation, in practice hex or octal
fn map_to_radix(s: &String, radix: usize, num_bits: u64) -> (String, ValueKind) {
    let mut is_undef = false;
    let mut is_highimp = false;
    let mut is_dontcare = false;
    let mut is_weak = false;
    let val = group_n_chars(
        &format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
        radix,
    )
    .into_iter()
    .map(|g| {
        if g.contains('x') {
            is_undef = true;
            "x".to_string()
        } else if g.contains('z') {
            is_highimp = true;
            "z".to_string()
        } else if g.contains('-') {
            is_dontcare = true;
            "-".to_string()
        } else if g.contains('u') {
            is_undef = true;
            "u".to_string()
        } else if g.contains('w') {
            is_undef = true;
            "w".to_string()
        } else if g.contains('h') {
            is_weak = true;
            "h".to_string()
        } else if g.contains('l') {
            is_weak = true;
            "l".to_string()
        } else {
            format!(
                "{:x}", // This works for radix up to 4, i.e., hex
                u8::from_str_radix(g, 2).expect("Found non-binary digit in value")
            )
        }
    })
    .join("");

    (
        val,
        if is_undef {
            ValueKind::Undef
        } else if is_highimp {
            ValueKind::HighImp
        } else if is_dontcare {
            ValueKind::DontCare
        } else if is_weak {
            ValueKind::Weak
        } else {
            ValueKind::Normal
        },
    )
}

fn check_wordlength(
    num_bits: Option<u32>,
    required: impl FnOnce(u32) -> bool,
) -> Result<TranslationPreference> {
    if let Some(num_bits) = num_bits {
        if required(num_bits) {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    } else {
        Ok(TranslationPreference::No)
    }
}

pub struct HexTranslator {}

impl BasicTranslator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$x}", width = no_of_digits(num_bits, 4)),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => map_to_radix(s, 4, num_bits),
        }
    }
}

pub struct BitTranslator {}

impl BasicTranslator for BitTranslator {
    fn name(&self) -> String {
        String::from("Bit")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                if (*v).is_zero() {
                    "0".to_string()
                } else {
                    "1".to_string()
                },
                ValueKind::Normal,
            ),
            SignalValue::String(s) => (s.to_string(), color_for_binary_representation(s)),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        if let Some(num_bits) = signal.num_bits {
            if num_bits == 1u32 {
                Ok(TranslationPreference::Prefer)
            } else {
                Ok(TranslationPreference::No)
            }
        } else {
            Ok(TranslationPreference::No)
        }
    }

    fn signal_info(&self, _signal: &SignalMeta) -> Result<SignalInfo> {
        Ok(SignalInfo::Bool)
    }
}

pub struct OctalTranslator {}

impl BasicTranslator for OctalTranslator {
    fn name(&self) -> String {
        String::from("Octal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$o}", width = no_of_digits(num_bits, 3)),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => map_to_radix(s, 3, num_bits),
        }
    }
}

pub struct GroupingBinaryTranslator {}

impl BasicTranslator for GroupingBinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary (with groups)")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        let (val, color) = match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$b}", width = num_bits as usize),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => (
                format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
                color_for_binary_representation(s),
            ),
        };

        (group_n_chars(&val, 4).join(" "), color)
    }
}

pub struct BinaryTranslator {}

impl BasicTranslator for BinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$b}", width = num_bits as usize),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => (
                format!("{extra_bits}{s}", extra_bits = extend_string(s, num_bits)),
                color_for_binary_representation(s),
            ),
        }
    }
}

pub struct ASCIITranslator {}

impl BasicTranslator for ASCIITranslator {
    fn name(&self) -> String {
        String::from("ASCII")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                v.to_bytes_be()
                    .into_iter()
                    .map(|val| format!("{cval}", cval = val as char))
                    .join(""),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => match check_vector_signal(s) {
                Some(v) => v,
                None => (
                    group_n_chars(s, 8)
                        .into_iter()
                        .map(|substr| {
                            format!(
                                "{cval}",
                                cval = u8::from_str_radix(substr, 2).unwrap_or_else(|_| panic!(
                                    "Found non-binary digit {substr} in value"
                                )) as char
                            )
                        })
                        .join(""),
                    ValueKind::Normal,
                ),
            },
        }
    }
}

pub struct RiscvTranslator {}

impl BasicTranslator for RiscvTranslator {
    fn name(&self) -> String {
        "RV32I".to_string()
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        let u32_value = match value {
            SignalValue::BigUint(v) => v.to_u32_digits().last().cloned(),
            SignalValue::String(s) => match check_vector_signal(s) {
                Some(v) => return v,
                None => s.parse().ok(),
            },
        };

        match asm_riscv::I::try_from(u32_value.unwrap_or(0)) {
            Ok(insn) => (riscv_to_string(&insn).to_string(), ValueKind::Normal),
            Err(_) => ("UNKNOWN INSN".to_string(), ValueKind::Warn),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 32)
    }
}

fn riscv_to_string(i: &asm_riscv::I) -> String {
    match i {
        asm_riscv::I::LUI { d, im } => format!("lui {d:?}, {im:?}"),
        asm_riscv::I::AUIPC { d, im } => format!("auipc {d:?}, {im:?}"),
        asm_riscv::I::JAL { d, im } => format!("jal {d:?}, {im:?}"),
        asm_riscv::I::JALR { d, s, im } => format!("jalr {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::BEQ { s1, s2, im } => format!("beq {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::BNE { s1, s2, im } => format!("bne {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::BLT { s1, s2, im } => format!("blt {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::BGE { s1, s2, im } => format!("bge {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::BLTU { s1, s2, im } => format!("bltu {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::BGEU { s1, s2, im } => format!("bgeu {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::LB { d, s, im } => format!("lb {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::LH { d, s, im } => format!("lh {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::LW { d, s, im } => format!("lw {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::LBU { d, s, im } => format!("lbu {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::LHU { d, s, im } => format!("lhu {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SB { s1, s2, im } => format!("sb {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::SH { s1, s2, im } => format!("sh {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::SW { s1, s2, im } => format!("sw {s1:?}, {s2:?}, {im:?}"),
        asm_riscv::I::ADDI { d, s, im } => format!("addi {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SLTI { d, s, im } => format!("slti {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SLTUI { d, s, im } => format!("sltui {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::XORI { d, s, im } => format!("xori {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::ORI { d, s, im } => format!("ori {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::ANDI { d, s, im } => format!("andi {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SLLI { d, s, im } => format!("slli {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SRLI { d, s, im } => format!("srli {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::SRAI { d, s, im } => format!("srai {d:?}, {s:?}, {im:?}"),
        asm_riscv::I::ADD { d, s1, s2 } => format!("add {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SUB { d, s1, s2 } => format!("sub {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SLL { d, s1, s2 } => format!("sll {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SLT { d, s1, s2 } => format!("slt {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SLTU { d, s1, s2 } => format!("sltu {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::XOR { d, s1, s2 } => format!("xor {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SRL { d, s1, s2 } => format!("srl {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::SRA { d, s1, s2 } => format!("sra {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::OR { d, s1, s2 } => format!("or {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::AND { d, s1, s2 } => format!("and {d:?}, {s1:?}, {s2:?}"),
        asm_riscv::I::ECALL {} => "ecall".to_string(),
        asm_riscv::I::EBREAK {} => "ebreak".to_string(),
        asm_riscv::I::FENCE { im } => format!("fence {im:?}"),
    }
}

fn decode_lebxxx(value: &num::BigUint) -> Result<num::BigUint, &'static str> {
    let bytes = value.to_bytes_be();
    match bytes.first() {
        Some(b) if b & 0x80 != 0 => return Err("invalid MSB"),
        _ => (),
    };

    let first: num::BigUint = bytes.first().cloned().unwrap_or(0).into();
    bytes.iter().skip(1).try_fold(first, |result, b| {
        if (b & 0x80 == 0) != (result == 0u8.into()) {
            Err("invalid flag")
        } else {
            Ok((result << 7) + (*b & 0x7f))
        }
    })
}

pub struct LebTranslator {}

impl BasicTranslator for LebTranslator {
    fn name(&self) -> String {
        "LEBxxx".to_string()
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        let decoded = match value {
            SignalValue::BigUint(v) => decode_lebxxx(v),
            SignalValue::String(s) => match check_vector_signal(s) {
                Some(v) => return v,
                None => match num::BigUint::parse_bytes(s.as_bytes(), 2) {
                    Some(bi) => decode_lebxxx(&bi),
                    None => return ("INVALID".to_owned(), ValueKind::Warn),
                },
            },
        };

        match decoded {
            Ok(decoded) => (decoded.to_str_radix(10), ValueKind::Normal),
            Err(s) => (
                s.to_owned()
                    + ": "
                    + &GroupingBinaryTranslator {}
                        .basic_translate(num_bits, value)
                        .0,
                ValueKind::Warn,
            ),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_wordlength(signal.num_bits, |n| (n % 8 == 0) && n > 0)
    }
}

#[cfg(test)]
mod test {
    use spade_common::num_ext::InfallibleToBigUint;

    use super::*;

    #[test]
    fn hexadecimal_translation_groups_digits_correctly_string() {
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::String("10000".to_string()))
                .0,
            "10"
        );

        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::String("1000".to_string()))
                .0,
            "08"
        );

        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::String("100000".to_string()))
                .0,
            "20"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(10, &SignalValue::String("1z00x0".to_string()))
                .0,
            "0zx"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(10, &SignalValue::String("z0110".to_string()))
                .0,
            "zz6"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(24, &SignalValue::String("xz0110".to_string()))
                .0,
            "xxxxx6"
        );
    }

    #[test]
    fn hexadecimal_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b10000u32.to_biguint()))
                .0,
            "10"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b01000u32.to_biguint()))
                .0,
            "08"
        );
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0u32.to_biguint()))
                .0,
            "00"
        );
    }

    #[test]
    fn octal_translation_groups_digits_correctly_string() {
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &SignalValue::String("10000".to_string()))
                .0,
            "20"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &SignalValue::String("100".to_string()))
                .0,
            "04"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(9, &SignalValue::String("x100".to_string()))
                .0,
            "xx4"
        );
    }

    #[test]
    fn octal_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b10000u32.to_biguint()))
                .0,
            "20"
        );
        assert_eq!(
            OctalTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b00100u32.to_biguint()))
                .0,
            "04"
        );
    }

    #[test]
    fn grouping_binary_translation_groups_digits_correctly_string() {
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(5, &SignalValue::String("1000w".to_string()))
                .0,
            "1 000w"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(8, &SignalValue::String("100l00".to_string()))
                .0,
            "0010 0l00"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &SignalValue::String("10x00".to_string()))
                .0,
            "001 0x00"
        );
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &SignalValue::String("z10x00".to_string()))
                .0,
            "zz1 0x00"
        );
    }

    #[test]
    fn grouping_binary_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            GroupingBinaryTranslator {}
                .basic_translate(7, &SignalValue::BigUint(0b100000u32.to_biguint()))
                .0,
            "010 0000"
        )
    }

    #[test]
    fn binary_translation_groups_digits_correctly_string() {
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(5, &SignalValue::String("10000".to_string()))
                .0,
            "10000"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(8, &SignalValue::String("100h00".to_string()))
                .0,
            "00100h00"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &SignalValue::String("10x0-".to_string()))
                .0,
            "0010x0-"
        );
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &SignalValue::String("z10x00".to_string()))
                .0,
            "zz10x00"
        );
    }

    #[test]
    fn binary_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            BinaryTranslator {}
                .basic_translate(7, &SignalValue::BigUint(0b100000u32.to_biguint()))
                .0,
            "0100000"
        )
    }

    #[test]
    fn ascii_translation_from_biguint() {
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(15, &SignalValue::BigUint(0b100111101001011u32.to_biguint()))
                .0,
            "OK"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(72, &SignalValue::BigUint(0b010011000110111101101110011001110010000001110100011001010111001101110100u128.to_biguint()))
                .0,
            "Long test"
        );
    }

    #[test]
    fn ascii_translation_from_string() {
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(15, &SignalValue::String("100111101001011".to_string()))
                .0,
            "OK"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(
                    72,
                    &SignalValue::String(
                        "010011000110111101101110011001110010000001110100011001010111001101110100"
                            .to_string()
                    )
                )
                .0,
            "Long test"
        );
        assert_eq!(
            ASCIITranslator {}
                .basic_translate(16, &SignalValue::String("010x111101001011".to_string()))
                .0,
            "UNDEF"
        );
    }

    #[test]
    fn bit_translation_from_biguint() {
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &SignalValue::BigUint(0b1u8.to_biguint()))
                .0,
            "1"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &SignalValue::BigUint(0b0u8.to_biguint()))
                .0,
            "0"
        );
    }

    #[test]
    fn bit_translation_from_string() {
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &SignalValue::String("1".to_string()))
                .0,
            "1"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &SignalValue::String("0".to_string()))
                .0,
            "0"
        );
        assert_eq!(
            BitTranslator {}
                .basic_translate(1, &SignalValue::String("x".to_string()))
                .0,
            "x"
        );
    }

    #[test]
    fn leb_translation_from_biguint() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &SignalValue::BigUint(0b01011010_11101111u16.into()))
                .0,
            "11631"
        );
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &SignalValue::BigUint(0b00000000_00000001u16.into()))
                .0,
            "1"
        );
        assert_eq!(
            LebTranslator{}.basic_translate(64, &SignalValue::BigUint(0b01001010_11110111_11101000_10100000_10111010_11110110_11100001_10011001u64.into())).0, "42185246214303897"
        );
    }
    #[test]
    fn leb_translation_from_string() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &SignalValue::String("0111110011100010".to_owned()))
                .0,
            "15970"
        )
    }
    #[test]
    fn leb_translation_invalid_msb() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &SignalValue::BigUint(0b1000000010000000u16.into()))
                .0,
            "invalid MSB: 1000 0000 1000 0000"
        )
    }
    #[test]
    fn leb_translation_invalid_continuation() {
        assert_eq!(
            LebTranslator {}
                .basic_translate(16, &SignalValue::BigUint(0b0111111101111111u16.into()))
                .0,
            "invalid flag: 0111 1111 0111 1111"
        )
    }

    #[test]
    fn riscv_from_bigunit() {
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(32, &SignalValue::BigUint(0b1000000010000000u32.into()))
                .0,
            "UNKNOWN INSN"
        );
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(
                    32,
                    &SignalValue::BigUint(0b100000010011_01010_000_01011_0010011u32.into())
                )
                .0,
            "addi A1, A0, 2067"
        );
    }
    #[test]
    fn riscv_from_string() {
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(
                    32,
                    &SignalValue::String("01001000100010001000100010001000".to_owned())
                )
                .0,
            "UNKNOWN INSN"
        );
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(
                    32,
                    &SignalValue::String("01xzz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "UNDEF"
        );
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(
                    32,
                    &SignalValue::String("010zz-hlw0010001000100010001000".to_owned())
                )
                .0,
            "HIGHIMP"
        );
        assert_eq!(
            RiscvTranslator {}
                .basic_translate(
                    32,
                    &SignalValue::String("01011-hlw0010001000100010001000".to_owned())
                )
                .0,
            "DON'T CARE"
        );
        // assert_eq!(
        //    RiscvTranslator {}
        //        .basic_translate(32, &SignalValue::String("10000001001101000000001000010011".to_owned()))
        //        .0,
        //    "addi TP, S0, 2067"
        //);
    }
}
