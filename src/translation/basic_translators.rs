use super::{BasicTranslator, ValueColor};

use fastwave_backend::SignalValue;
use itertools::Itertools;
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

fn no_of_digits(num_bits: u64, digit_size: u64) -> usize {
    if (num_bits % digit_size) == 0 {
        (num_bits / digit_size) as usize
    } else {
        (num_bits / digit_size + 1) as usize
    }
}

fn extend_string(val: &String, num_bits: u64) -> String {
    // VCD spec'd bit extension
    let extra_bits = if num_bits > val.len() as u64 {
        let extra_count = num_bits - val.len() as u64;
        let extra_value = match val.chars().next() {
            Some('0') => "0",
            Some('1') => "0",
            Some('x') => "x",
            Some('z') => "z",
            // If we got weird characters, this is probably a string, so we don't
            // do the extension
            _ => "",
        };
        extra_value.repeat(extra_count as usize)
    } else {
        String::new()
    };
    return extra_bits;
}

pub struct HexTranslator {}

impl BasicTranslator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$x}", width = no_of_digits(num_bits, 4)),
                ValueColor::Normal,
            ),
            SignalValue::String(s) => {
                let mut is_undef = false;
                let mut is_highimp = false;
                let extra_bits = extend_string(&s, num_bits);
                let val = group_n_chars(&format!("{extra_bits}{s}"), 4)
                    .into_iter()
                    .map(|g| {
                        if g.contains('x') {
                            is_undef = true;
                            "x".to_string()
                        } else if g.contains('z') {
                            is_highimp = true;
                            "z".to_string()
                        } else {
                            format!(
                                "{:x}",
                                u8::from_str_radix(g, 2).expect("Found non-binary digit in value")
                            )
                        }
                    })
                    .join("");

                (
                    val,
                    if is_undef {
                        ValueColor::Undef
                    } else if is_highimp {
                        ValueColor::HighImp
                    } else {
                        ValueColor::Normal
                    },
                )
            }
        }
    }
}

pub struct OctalTranslator {}

impl BasicTranslator for OctalTranslator {
    fn name(&self) -> String {
        String::from("Octal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$o}", width = no_of_digits(num_bits, 3)),
                ValueColor::Normal,
            ),
            SignalValue::String(s) => {
                let mut is_undef = false;
                let mut is_highimp = false;
                let extra_bits = extend_string(&s, num_bits);
                let val = group_n_chars(&format!("{extra_bits}{s}"), 3)
                    .into_iter()
                    .map(|g| {
                        if g.contains('x') {
                            is_undef = true;
                            "x".to_string()
                        } else if g.contains('z') {
                            is_highimp = true;
                            "z".to_string()
                        } else {
                            format!(
                                "{:o}",
                                u8::from_str_radix(g, 2).expect("Found non-binary digit in value")
                            )
                        }
                    })
                    .join("");

                (
                    val,
                    if is_undef {
                        ValueColor::Undef
                    } else if is_highimp {
                        ValueColor::HighImp
                    } else {
                        ValueColor::Normal
                    },
                )
            }
        }
    }
}

pub struct UnsignedTranslator {}

impl BasicTranslator for UnsignedTranslator {
    fn name(&self) -> String {
        String::from("Unsigned")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (format!("{v}"), ValueColor::Normal),
            SignalValue::String(s) => {
                if s.contains("x") {
                    (format!("UNDEF"), ValueColor::Undef)
                } else if s.contains("z") {
                    (format!("HIGHIMP"), ValueColor::HighImp)
                } else {
                    (
                        u128::from_str_radix(s, 2)
                            .map(|val| format!("{val}"))
                            .unwrap_or(s.clone()),
                        ValueColor::Normal,
                    )
                }
            }
        }
    }
}

pub struct SignedTranslator {}

impl BasicTranslator for SignedTranslator {
    fn name(&self) -> String {
        String::from("Signed")
    }

    fn basic_translate(&self, _num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => {
                let signweight = 1u32.to_biguint() << (_num_bits - 1);
                if *v < signweight {
                    (format!("{v}"), ValueColor::Normal)
                } else {
                    let v2 = (signweight << 1) - v;
                    (format!("-{v2}"), ValueColor::Normal)
                }
            }
            SignalValue::String(s) => {
                if s.contains("x") {
                    (format!("UNDEF"), ValueColor::Undef)
                } else if s.contains("z") {
                    (format!("HIGHIMP"), ValueColor::HighImp)
                } else {
                    let v = u128::from_str_radix(s, 2).expect("Cannot parse");
                    let signweight = 1u128 << (_num_bits - 1);
                    if v < signweight {
                        (format!("{v}"), ValueColor::Normal)
                    } else {
                        let v2 = (signweight << 1) - v;
                        (format!("-{v2}"), ValueColor::Normal)
                    }
                }
            }
        }
    }
}

pub struct ExtendingBinaryTranslator {}

impl BasicTranslator for ExtendingBinaryTranslator {
    fn name(&self) -> String {
        String::from("Binary (with extension)")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        let (val, color) = match value {
            SignalValue::BigUint(v) => (format!("{v:b}"), ValueColor::Normal),
            SignalValue::String(s) => {
                let val = s.clone();
                if s.contains("x") {
                    (val, ValueColor::Undef)
                } else if s.contains("z") {
                    (val, ValueColor::HighImp)
                } else {
                    (val, ValueColor::Normal)
                }
            }
        };

        let extra_bits = extend_string(&val, num_bits);

        (
            group_n_chars(&format!("{extra_bits}{val}"), 4).join(" "),
            color,
        )
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
    fn binary_translation_groups_digits_correctly_string() {
        assert_eq!(
            ExtendingBinaryTranslator {}
                .basic_translate(5, &SignalValue::String("10000".to_string()))
                .0,
            "1 0000"
        );
        assert_eq!(
            ExtendingBinaryTranslator {}
                .basic_translate(5, &SignalValue::String("100000".to_string()))
                .0,
            "10 0000"
        );
        assert_eq!(
            ExtendingBinaryTranslator {}
                .basic_translate(7, &SignalValue::String("10x00".to_string()))
                .0,
            "001 0x00"
        );
        assert_eq!(
            ExtendingBinaryTranslator {}
                .basic_translate(7, &SignalValue::String("z10x00".to_string()))
                .0,
            "zz1 0x00"
        );
    }

    #[test]
    fn binary_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            ExtendingBinaryTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b100000u32.to_biguint()))
                .0,
            "10 0000"
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
}
