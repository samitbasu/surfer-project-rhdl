use std::fmt;

use fastwave_backend::Timescale;
use serde::{
    de::{self, Visitor},
    Deserializer, Serializer,
};

pub fn serialize_timescale<S>(timescale: &Timescale, ser: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    match timescale {
        Timescale::Fs => ser.serialize_str("fs"),
        Timescale::Ps => ser.serialize_str("ps"),
        Timescale::Ns => ser.serialize_str("ns"),
        Timescale::Us => ser.serialize_str("us"),
        Timescale::Ms => ser.serialize_str("ms"),
        Timescale::S => ser.serialize_str("s"),
        Timescale::Unit => ser.serialize_str("unit"),
    }
}

struct TimeScaleVisitor;

impl<'de> Visitor<'de> for TimeScaleVisitor {
    type Value = Timescale;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected a timescale")
    }

    fn visit_str<E>(self, s: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match s {
            "fs" => Ok(Timescale::Fs),
            "ps" => Ok(Timescale::Ps),
            "ns" => Ok(Timescale::Ns),
            "us" => Ok(Timescale::Us),
            "ms" => Ok(Timescale::Ms),
            "s" => Ok(Timescale::S),
            "unit" => Ok(Timescale::Unit),
            other => Err(E::custom(format!("{other} not a valid timescale"))),
        }
    }
}

pub fn deserialize_timescale<'de, D>(de: D) -> Result<Timescale, D::Error>
where
    D: Deserializer<'de>,
{
    de.deserialize_str(TimeScaleVisitor)
}
