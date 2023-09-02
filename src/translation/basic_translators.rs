use super::{BasicTranslator, ValueColor};

use fastwave_backend::SignalValue;
use itertools::Itertools;
use spade_common::num_ext::InfallibleToBigUint;

// Forms groups of 4 chars from from a string. If the string size is
// not divisible by 4, the first group will be smaller than 4
// The string must only consist of ascii characters
fn group_4_chars<'a>(s: &'a str) -> Vec<&'a str> {
    let num_extra_chars = s.len() % 4;

    let last_group = &s[0..num_extra_chars];

    let rest_groups = s.len() / 4;
    let rest_str = &s[num_extra_chars..];

    if !last_group.is_empty() {
        vec![last_group]
    } else {
        vec![]
    }
    .into_iter()
    .chain((0..rest_groups).map(|start| &rest_str[start * 4..(start + 1) * 4]))
    .collect()
}

pub struct HexTranslator {}

impl BasicTranslator for HexTranslator {
    fn name(&self) -> String {
        String::from("Hexadecimal")
    }

    fn basic_translate(&self, num_bits: u64, value: &SignalValue) -> (String, ValueColor) {
        match value {
            SignalValue::BigUint(v) => (
                format!("{v:0width$x}", width = num_bits as usize / 4),
                ValueColor::Normal,
            ),
            SignalValue::String(s) => {
                let mut is_undef = false;
                let mut is_highimp = false;

                let val = group_4_chars(s)
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
                                u8::from_str_radix(&g, 2).expect("Found non binary digit in value")
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

        (
            group_4_chars(&format!("{extra_bits}{val}")).join(" "),
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
                .basic_translate(5, &SignalValue::String("100000".to_string()))
                .0,
            "20"
        );
    }

    #[test]
    fn hexadecimal_translation_groups_digits_correctly_bigint() {
        assert_eq!(
            HexTranslator {}
                .basic_translate(5, &SignalValue::BigUint(0b10000u32.to_biguint()))
                .0,
            "10"
        )
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
        )
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
