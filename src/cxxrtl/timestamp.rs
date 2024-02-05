use std::str::FromStr;

use color_eyre::{eyre::Context, Result};
use num::{BigUint, Zero};
use serde::{Deserialize, Deserializer, Serialize};
use spade_common::num_ext::InfallibleToBigUint;

#[derive(Debug, Clone)]
pub struct CxxrtlTimestamp {
    seconds: BigUint,
    femtoseconds: BigUint,
}

impl CxxrtlTimestamp {
    pub fn zero() -> Self {
        Self {
            seconds: BigUint::zero(),
            femtoseconds: BigUint::zero(),
        }
    }

    fn from_str(s: &str) -> Result<Self> {
        let split = s.split(".").collect::<Vec<_>>();

        Ok(CxxrtlTimestamp {
            seconds: BigUint::from_str(split[0])
                .with_context(|| format!("When parsing seconds from {s}"))?,
            femtoseconds: BigUint::from_str(split[1])
                .with_context(|| format!("When parsing femtoseconds from {s}"))?,
        })
    }

    pub fn to_string(&self) -> String {
        format!("{}.{}", self.seconds, self.femtoseconds)
    }

    pub fn from_femtoseconds(femto: BigUint) -> Self {
        Self {
            // TODO: Stop using the spade function here
            seconds: &femto / 1_000_000_000_000_000u64.to_biguint(),
            femtoseconds: &femto % 1_000_000_000_000_000u64.to_biguint(),
        }
    }

    pub fn into_femtoseconds(&self) -> BigUint {
        &self.seconds * 1_000_000_000_000_000u64.to_biguint() + &self.femtoseconds
    }
}

impl<'de> Deserialize<'de> for CxxrtlTimestamp {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let buf = String::deserialize(deserializer)?;

        Self::from_str(&buf).map_err(serde::de::Error::custom)
    }
}

impl Serialize for CxxrtlTimestamp {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_string().serialize(serializer)
    }
}
