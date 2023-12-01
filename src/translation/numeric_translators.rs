use color_eyre::Result;
use fastwave_backend::SignalValue;
use half::{bf16, f16};
use num::BigUint;
use softposit::{P16E1, P32E2, P8E0, Q16E1, Q8E0};
use spade_common::num_ext::InfallibleToBigUint;

use crate::wave_container::SignalMeta;

use super::{
    check_single_wordlength, map_vector_signal, translates_all_bit_types, BasicTranslator,
    NumberParseResult, TranslationPreference, ValueKind,
};

pub trait NumericTranslator {
    fn name(&self) -> String;
    fn translate_biguint(&self, _: u64, _: BigUint) -> String;
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        translates_all_bit_types(signal)
    }
}

impl<T: NumericTranslator> BasicTranslator for T {
    fn name(&self) -> String {
        self.name()
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueKind) {
        match value {
            SignalValue::BigUint(v) => (
                self.translate_biguint(num_bits, v.clone()),
                ValueKind::Normal,
            ),
            SignalValue::String(s) => match map_vector_signal(s) {
                NumberParseResult::Unparsable(v, k) => (v, k),
                NumberParseResult::Numerical(v) => {
                    (self.translate_biguint(num_bits, v), ValueKind::Normal)
                }
            },
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        self.translates(signal)
    }
}

pub struct UnsignedTranslator {}

impl NumericTranslator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        format!("{v}")
    }
}

pub struct SignedTranslator {}

impl NumericTranslator for SignedTranslator {
    fn name(&self) -> String {
        String::from("Signed")
    }

    fn translate_biguint(&self, num_bits: u64, v: num::BigUint) -> String {
        let signweight = 1u32.to_biguint() << (num_bits - 1);
        if v < signweight {
            format!("{v}")
        } else {
            let v2 = (signweight << 1) - v;
            format!("-{v2}")
        }
    }
}

pub struct SinglePrecisionTranslator {}

impl NumericTranslator for SinglePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 32-bit IEEE 754")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{fp:e}", fp = f32::from_bits(val)),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 32)
    }
}

pub struct DoublePrecisionTranslator {}

impl NumericTranslator for DoublePrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 64-bit IEEE 754")
    }
    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u64_digits().next() {
            Some(val) => format!("{fp:e}", fp = f64::from_bits(val)),
            None => "Unknown".to_string(),
        }
    }
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 64)
    }
}

#[cfg(feature = "f128")]
pub struct QuadPrecisionTranslator {}

#[cfg(feature = "f128")]
impl NumericTranslator for QuadPrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 128-bit IEEE 754")
    }
    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        let mut digits = v.iter_u64_digits();
        let lsb = digits.next().unwrap_or(0);
        let msb = if digits.len() > 0 {
            digits.next().unwrap_or(0)
        } else {
            0
        };
        let val = lsb as u128 | (msb as u128) << 64;
        f128::f128::from_bits(val).to_string()
    }
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 128)
    }
}

pub struct HalfPrecisionTranslator {}

impl NumericTranslator for HalfPrecisionTranslator {
    fn name(&self) -> String {
        String::from("FP: 16-bit IEEE 754")
    }
    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{fp:e}", fp = f16::from_bits(val as u16)),
            None => "Unknown".to_string(),
        }
    }
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 16)
    }
}

pub struct BFloat16Translator {}

impl NumericTranslator for BFloat16Translator {
    fn name(&self) -> String {
        String::from("FP: bfloat16")
    }
    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{fp:e}", fp = bf16::from_bits(val as u16)),
            None => "Unknown".to_string(),
        }
    }
    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 16)
    }
}

pub struct Posit32Translator {}

impl NumericTranslator for Posit32Translator {
    fn name(&self) -> String {
        String::from("Posit: 32-bit (two exponent bits)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{p}", p = P32E2::from_bits(val)),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 32)
    }
}

pub struct Posit16Translator {}

impl NumericTranslator for Posit16Translator {
    fn name(&self) -> String {
        String::from("Posit: 16-bit (one exponent bit)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{p}", p = P16E1::from_bits(val as u16)),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 16)
    }
}

pub struct Posit8Translator {}

impl NumericTranslator for Posit8Translator {
    fn name(&self) -> String {
        String::from("Posit: 8-bit (no exponent bit)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{p}", p = P8E0::from_bits(val as u8)),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 8)
    }
}

pub struct PositQuire8Translator {}

impl NumericTranslator for PositQuire8Translator {
    fn name(&self) -> String {
        String::from("Posit: quire for 8-bit (no exponent bit)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => format!("{p}", p = Q8E0::from_bits(val)),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 32)
    }
}

pub struct PositQuire16Translator {}

impl NumericTranslator for PositQuire16Translator {
    fn name(&self) -> String {
        String::from("Posit: quire for 16-bit (one exponent bit)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        let mut digits = v.iter_u64_digits();
        let lsb = digits.next().unwrap_or(0);
        let msb = if digits.len() > 0 {
            digits.next().unwrap_or(0)
        } else {
            0
        };
        let val = lsb as u128 | (msb as u128) << 64;
        format!("{p}", p = Q16E1::from_bits(val))
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 128)
    }
}

/// Decode u8 as 8-bit float with five exponent bits and two mantissa bits
fn decode_e5m2(v: u8) -> String {
    let mant = v & 3;
    let exp = v >> 2 & 31;
    let sign: i8 = 1 - (v >> 6) as i8; // 1 - 2*signbit
    match (exp, mant) {
        (31, 0) => "∞".to_string(),
        (31, ..) => "NaN".to_string(),
        (0, 0) => "0.0".to_string(),
        (0, ..) => format!(
            "{fp:e}",
            fp = ((sign * mant as i8) as f32) * 0.0000152587890625f32 // 0.0000152587890625 = 2^-16
        ),
        _ => format!(
            "{fp:e}",
            fp = ((sign * (4 + mant as i8)) as f32) * 2.0f32.powi(exp as i32 - 17) // 17 = 15 (bias) + 2 (mantissa bits)
        ),
    }
}

pub struct E5M2Translator {}

impl NumericTranslator for E5M2Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E5M2)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => decode_e5m2(val as u8),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 8)
    }
}

/// Decode u8 as 8-bit float with four exponent bits and three mantissa bits
fn decode_e4m3(v: u8) -> String {
    let mant = v & 7;
    let exp = v >> 3 & 15;
    let sign: i8 = 1 - (v >> 6) as i8; // 1 - 2*signbit
    match (exp, mant) {
        (15, 7) => "NaN".to_string(),
        (0, 0) => "0.0".to_string(),
        (0, ..) => format!("{fp:e}", fp = ((sign * mant as i8) as f32) * 0.001953125f32), // 0.001953125 = 2^-9
        _ => format!(
            "{fp:e}",
            fp = ((sign * (8 + mant) as i8) as f32) * 2.0f32.powi(exp as i32 - 10) // 10 = 7 (bias) + 3 (mantissa bits)
        ),
    }
}

pub struct E4M3Translator {}

impl NumericTranslator for E4M3Translator {
    fn name(&self) -> String {
        String::from("FP: 8-bit (E4M3)")
    }

    fn translate_biguint(&self, _: u64, v: num::BigUint) -> String {
        match v.iter_u32_digits().next() {
            Some(val) => decode_e4m3(val as u8),
            None => "Unknown".to_string(),
        }
    }

    fn translates(&self, signal: &SignalMeta) -> Result<TranslationPreference> {
        check_single_wordlength(signal.num_bits, 8)
    }
}

#[cfg(test)]
mod test {
    use spade_common::num_ext::InfallibleToBigUint;

    use super::*;

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
    fn posit8_translation_from_biguint() {
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &SignalValue::BigUint(0b10001000u8.to_biguint()))
                .0,
            "-8"
        );
    }

    #[test]
    fn posit8_translation_from_string() {
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &SignalValue::String("11111111".to_string()))
                .0,
            "-0.015625"
        );
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &SignalValue::String("00000011".to_string()))
                .0,
            "0.046875"
        );
        assert_eq!(
            Posit8Translator {}
                .basic_translate(8, &SignalValue::String("10000000".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn posit16_translation_from_biguint() {
        assert_eq!(
            Posit16Translator {}
                .basic_translate(
                    16,
                    &SignalValue::BigUint(0b1010101010001000u16.to_biguint())
                )
                .0,
            "-2.68359375"
        );
    }

    #[test]
    fn posit16_translation_from_string() {
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &SignalValue::String("1111111111111111".to_string()))
                .0,
            "-0.000000003725290298461914"
        );
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &SignalValue::String("0111000000000011".to_string()))
                .0,
            "16.046875"
        );
        assert_eq!(
            Posit16Translator {}
                .basic_translate(16, &SignalValue::String("1000000000000000".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn posit32_translation_from_biguint() {
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &SignalValue::BigUint(0b1010101010001000u16.to_biguint())
                )
                .0,
            "0.0000000000000000023056236824262055"
        );
    }

    #[test]
    fn posit32_translation_from_string() {
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &SignalValue::String("10000111000000001111111111111111".to_string())
                )
                .0,
            "-8176.000244140625"
        );
        assert_eq!(
            Posit32Translator {}
                .basic_translate(
                    32,
                    &SignalValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "257.75"
        );
    }

    #[test]
    fn quire8_translation_from_biguint() {
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &SignalValue::BigUint(0b1010101010001000u16.to_biguint())
                )
                .0,
            "10"
        );
    }

    #[test]
    fn quire8_translation_from_string() {
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &SignalValue::String("10000111000000001111111111111111".to_string())
                )
                .0,
            "-64"
        );
        assert_eq!(
            PositQuire8Translator {}
                .basic_translate(
                    32,
                    &SignalValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "64"
        );
    }

    #[test]
    fn quire16_translation_from_biguint() {
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(128, &SignalValue::BigUint(0b10101010100010001010101010001000101010101000100010101010100010001010101010001000101010101000100010101010100010001010101010001000u128.to_biguint()))
                .0,
            "-268435456"
        );
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(128, &SignalValue::BigUint(7u8.to_biguint()))
                .0,
            "0.000000003725290298461914"
        );
    }

    #[test]
    fn quire16_translation_from_string() {
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(
                    128,
                    &SignalValue::String(
                        "1000011100000000111111111111111101110000000000111000000000000000"
                            .to_string()
                    )
                )
                .0,
            "135"
        );
        assert_eq!(
            PositQuire16Translator {}
                .basic_translate(
                    128,
                    &SignalValue::String("01110000000000111000000000000000".to_string())
                )
                .0,
            "0.000000029802322387695313"
        );
    }

    #[test]
    fn bloat16_translation_from_string() {
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &SignalValue::String("0100100011100011".to_string()))
                .0,
            "4.64896e5"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &SignalValue::String("1000000000000000".to_string()))
                .0,
            "-0e0"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(16, &SignalValue::String("1111111111111111".to_string()))
                .0,
            "NaN"
        );
    }

    #[test]
    fn bloat16_translation_from_bigunit() {
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &SignalValue::BigUint(0b1010101010001000u16.to_biguint())
                )
                .0,
            "-2.4158453e-13"
        );
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &SignalValue::BigUint(0b1000000000000000u16.to_biguint())
                )
                .0,
            "-0e0"
        );
        //assert_eq!(
        //    BFloat16Translator {}
        //        .basic_translate(16, &SignalValue::BigUint(0b0000000000000000u16.to_biguint()))
        //        .0,
        //    "0e0"
        //);
        assert_eq!(
            BFloat16Translator {}
                .basic_translate(
                    16,
                    &SignalValue::BigUint(0b1111111111111111u16.to_biguint())
                )
                .0,
            "NaN"
        );
    }

    #[test]
    fn half_translation_from_string() {
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &SignalValue::String("0100100011100011".to_string()))
                .0,
            "9.7734375e0"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &SignalValue::String("1000000000000000".to_string()))
                .0,
            "-0e0"
        );
        assert_eq!(
            HalfPrecisionTranslator {}
                .basic_translate(16, &SignalValue::String("1111111111111111".to_string()))
                .0,
            "NaN"
        );
    }
}
