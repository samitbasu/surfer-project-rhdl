use super::{BasicTranslator, TranslationPreference, ValueKind};

use color_eyre::Result;
use fastwave_backend::{Signal, SignalValue};
use half::{bf16, f16};
use itertools::Itertools;
use num::{ToPrimitive, Zero};
use softposit::p16e1::P16E1;
use softposit::p32e2::P32E2;
use softposit::p8e0::P8E0;
use spade_common::num_ext::InfallibleToBigInt;
use spade_common::num_ext::InfallibleToBigUint;

// Forms groups of n chars from from a string. If the string size is
// not divisible by n, the first group will be smaller than n
// The string must only consist of ascii characters
fn group_n_chars<'a>(s: &'a str, n: usize) -> Vec<&'a str> {
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
fn map_vector_signal(s: &str) -> Option<(String, ValueKind)> {
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
    } else {
        None
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

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 1u16 {
            Ok(TranslationPreference::Prefer)
        } else {
            Ok(TranslationPreference::No)
        }
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

pub struct UnsignedTranslator {}

impl BasicTranslator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (format!("{v}"), ValueKind::Normal),
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => (
                    u128::from_str_radix(s, 2)
                        .map(|val| format!("{val}"))
                        .unwrap_or(s.clone()),
                    ValueKind::Normal,
                ),
            },
        }
    }

    fn basic_numerical_range(&self, num_bits: u64) -> Option<(f64, f64)> {
        if let Some(maxpow) = (1u32.to_biguint() << num_bits).to_f64() {
            Some((0.0, maxpow - 1.0))
        } else {
            None
        }
    }

    fn basic_numerical_val(&self, _num_bits: u64, value: &SignalValue) -> Option<f64> {
        match value {
            SignalValue::BigUint(v) => v.to_f64(),
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(_) => None,
                None => u128::from_str_radix(s, 2).unwrap().to_f64(),
            },
        }
    }
}

pub struct SignedTranslator {}

impl BasicTranslator for SignedTranslator {
    fn name(&self) -> String {
        String::from("Signed")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => {
                let signweight = 1u32.to_biguint() << (_num_bits - 1);
                if *v < signweight {
                    (format!("{v}"), ValueKind::Normal)
                } else {
                    let v2 = (signweight << 1) - v;
                    (format!("-{v2}"), ValueKind::Normal)
                }
            }
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u128::from_str_radix(s, 2).expect("Cannot parse");
                    let signweight = 1u128 << (_num_bits - 1);
                    if v < signweight {
                        (format!("{v}"), ValueKind::Normal)
                    } else {
                        let v2 = (signweight << 1) - v;
                        (format!("-{v2}"), ValueKind::Normal)
                    }
                }
            },
        }
    }

    fn basic_numerical_range(&self, num_bits: u64) -> Option<(f64, f64)> {
        if let Some(signweight) = (1u32.to_bigint() << (num_bits - 1)).to_f64() {
            Some((-signweight, signweight - 1.0))
        } else {
            None
        }
    }

    fn basic_numerical_val(&self, num_bits: u64, value: &SignalValue) -> Option<f64> {
        match value {
            SignalValue::BigUint(v) => {
                let signweight = 1u32.to_biguint() << (num_bits - 1);
                if *v < signweight {
                    v.to_f64()
                } else {
                    (v.to_bigint() - 2u16 * signweight.to_bigint()).to_f64()
                }
            }
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(_) => None,
                None => {
                    let v = u128::from_str_radix(s, 2).expect("Cannot decode");
                    let signweight = 1u128 << (num_bits - 1);
                    if v < signweight {
                        v.to_f64()
                    } else {
                        (v as i128 - (signweight << 1) as i128).to_f64()
                    }
                }
            },
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
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => (
                    group_n_chars(s, 8)
                        .into_iter()
                        .map(|substr| {
                            format!(
                                "{cval}",
                                cval = u8::from_str_radix(substr, 2)
                                    .expect("Found non-binary digit in value")
                                    as char
                            )
                        })
                        .join(""),
                    ValueKind::Normal,
                ),
            },
        }
    }
}

pub struct SinglePrecisionTranslator {}

impl BasicTranslator for SinglePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 32-bit IEEE 754")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => (
                    format!("{fp:e}", fp = f32::from_bits(val)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u32::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{fp:e}", fp = f32::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 32u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct DoublePrecisionTranslator {}

impl BasicTranslator for DoublePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 64-bit IEEE 754")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u64_digits().last() {
                Some(val) => (
                    format!("{fp:e}", fp = f64::from_bits(val)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u64::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{fp:e}", fp = f64::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 64u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct HalfPrecisionTranslator {}

impl BasicTranslator for HalfPrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 16-bit IEEE 754")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => (
                    format!("{fp:e}", fp = f16::from_bits(val as u16)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u16::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{fp:e}", fp = f16::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 16u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct BFloat16Translator {}

impl BasicTranslator for BFloat16Translator {
    fn name(&self) -> String {
        String::from("FP: bfloat16")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u64_digits().last() {
                Some(val) => (
                    format!("{fp:e}", fp = bf16::from_bits(val as u16)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u16::from_str_radix(s, 2).expect("Cannot parse");
                    (
                        format!("{fp:e}", fp = bf16::from_bits(v)),
                        ValueKind::Normal,
                    )
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 16u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct Posit32Translator {}

impl BasicTranslator for Posit32Translator {
    fn name(&self) -> String {
        String::from("Posit: 32-bit (two exponent bits)")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => (format!("{p}", p = P32E2::from_bits(val)), ValueKind::Normal),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u32::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{p}", p = P32E2::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 32u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct Posit16Translator {}

impl BasicTranslator for Posit16Translator {
    fn name(&self) -> String {
        String::from("Posit: 16-bit (one exponent bit)")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => (
                    format!("{p}", p = P16E1::from_bits(val as u16)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u16::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{p}", p = P16E1::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 16u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

pub struct Posit8Translator {}

impl BasicTranslator for Posit8Translator {
    fn name(&self) -> String {
        String::from("Posit: 8-bit (no exponent bit)")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => (
                    format!("{p}", p = P8E0::from_bits(val as u8)),
                    ValueKind::Normal,
                ),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => {
                    let v = u8::from_str_radix(s, 2).expect("Cannot parse");
                    (format!("{p}", p = P8E0::from_bits(v)), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 8u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

/// Decode u8 as 8-bit float with five exponent bits and two mantissa bits
fn decode_e5m2(v: u8) -> (String, ValueKind) {
    let mant = v & 3;
    let exp = v >> 2 & 31;
    let sign: i8 = 1 - (v >> 6) as i8; // 1 - 2*signbit
    match (exp, mant) {
        (31, 0) => ("∞".to_string(), ValueKind::Normal),
        (31, ..) => ("NaN".to_string(), ValueKind::Normal),
        (0, 0) => ("0.0".to_string(), ValueKind::Normal),
        (0, ..) => (
            format!(
                "{fp:e}",
                fp = ((sign * mant as i8) as f32) * 0.0000152587890625f32 // 0.0000152587890625 = 2^-16
            ),
            ValueKind::Normal,
        ),
        _ => (
            format!(
                "{fp:e}",
                fp = ((sign * (4 + mant as i8)) as f32) * 2.0f32.powi(exp as i32 - 17) // 17 = 15 (bias) + 2 (mantissa bits)
            ),
            ValueKind::Normal,
        ),
    }
}

pub struct E5M2Translator {}

impl BasicTranslator for E5M2Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E5M2)")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => decode_e5m2(val as u8),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => decode_e5m2(u8::from_str_radix(s, 2).expect("Cannot parse")),
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 8u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
    }
}

/// Decode u8 as 8-bit float with four exponent bits and three mantissa bits
fn decode_e4m3(v: u8) -> (String, ValueKind) {
    let mant = v & 7;
    let exp = v >> 3 & 15;
    let sign: i8 = 1 - (v >> 6) as i8; // 1 - 2*signbit
    match (exp, mant) {
        (15, 7) => ("NaN".to_string(), ValueKind::Normal),
        (0, 0) => ("0.0".to_string(), ValueKind::Normal),
        (0, ..) => (
            format!("{fp:e}", fp = ((sign * mant as i8) as f32) * 0.001953125f32), // 0.001953125 = 2^-9
            ValueKind::Normal,
        ),
        _ => (
            format!(
                "{fp:e}",
                fp = ((sign * (8 + mant) as i8) as f32) * 2.0f32.powi(exp as i32 - 10) // 10 = 7 (bias) + 3 (mantissa bits)
            ),
            ValueKind::Normal,
        ),
    }
}

pub struct E4M3Translator {}

impl BasicTranslator for E4M3Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E4M3)")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => match v.iter_u32_digits().last() {
                Some(val) => decode_e4m3(val as u8),
                None => ("Unknown".to_string(), ValueKind::Normal),
            },
            SignalValue::String(s) => match map_vector_signal(s) {
                Some(v) => v,
                None => decode_e4m3(u8::from_str_radix(s, 2).expect("Cannot parse")),
            },
        }
    }

    fn translates(&self, signal: &Signal) -> Result<TranslationPreference> {
        if signal.num_bits().unwrap() == 8u16 {
            Ok(TranslationPreference::Yes)
        } else {
            Ok(TranslationPreference::No)
        }
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
    fn signed_translation_from_string() {
        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &SignalValue::String("10000".to_string()))
                .0,
            "-16"
        );

        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &SignalValue::String("01000".to_string()))
                .0,
            "8"
        );
    }

    #[test]
    fn signed_translation_from_biguint() {
        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b10011u32.to_biguint()))
                .0,
            "-13"
        );

        assert_eq!(
            SignedTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b01000u32.to_biguint()))
                .0,
            "8"
        );
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
    fn e4m3_translation_from_biguint() {
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &SignalValue::BigUint(0b10001000u8.to_biguint()))
                .0,
            "-1.5625e-2"
        );
    }

    #[test]
    fn e4m3_translation_from_string() {
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &SignalValue::String("11111111".to_string()))
                .0,
            "NaN"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &SignalValue::String("00000011".to_string()))
                .0,
            "5.859375e-3"
        );
        assert_eq!(
            E4M3Translator {}
                .basic_translate(8, &SignalValue::String("10000000".to_string()))
                .0,
            "0.0"
        );
    }

    #[test]
    fn e5m2_translation_from_biguint() {
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &SignalValue::BigUint(0b10000100u8.to_biguint()))
                .0,
            "-6.1035156e-5"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &SignalValue::BigUint(0b11111100u8.to_biguint()))
                .0,
            "∞"
        );
    }

    #[test]
    fn e5m2_translation_from_string() {
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &SignalValue::String("11111111".to_string()))
                .0,
            "NaN"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &SignalValue::String("00000011".to_string()))
                .0,
            "4.5776367e-5"
        );
        assert_eq!(
            E5M2Translator {}
                .basic_translate(8, &SignalValue::String("10000000".to_string()))
                .0,
            "0.0"
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
    fn numerical_range() {
        assert_eq!(
            UnsignedTranslator {}.basic_numerical_range(7),
            Some((0.0, 127.0))
        );
        assert_eq!(
            SignedTranslator {}.basic_numerical_range(7),
            Some((-64.0, 63.0))
        );
    }

    #[test]
    fn numerical_value() {
        assert_eq!(
            UnsignedTranslator {}
                .basic_numerical_val(7, &SignalValue::String("00000011".to_string())),
            Some(3.0)
        );
        assert_eq!(
            SignedTranslator {}
                .basic_numerical_val(7, &SignalValue::String("00000011".to_string())),
            Some(3.0)
        );
        assert_eq!(
            SignedTranslator {}.basic_numerical_val(7, &SignalValue::String("1111101".to_string())),
            Some(-3.0)
        );
    }
}
