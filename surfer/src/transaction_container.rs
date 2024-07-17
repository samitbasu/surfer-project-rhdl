use crate::time::{TimeScale, TimeUnit};
use crate::wave_container::MetaData;
use ftr_parser::types::FTR;
use num::BigUint;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::ops::Not;

pub struct TransactionContainer {
    pub inner: FTR,
}

impl TransactionContainer {
    pub fn stream_scope_exists(&self, stream_scope: &StreamScopeRef) -> bool {
        match stream_scope {
            StreamScopeRef::Root => true,
            StreamScopeRef::Stream(s) => self.inner.tx_streams.contains_key(&s.stream_id),
        }
    }

    pub fn stream_names(&self) -> Vec<String> {
        let mut names = vec![String::from("tr")];
        let mut stream_names: Vec<String> = self
            .inner
            .tx_streams
            .iter()
            .map(|(_, s)| s.name.clone())
            .collect();
        names.append(&mut stream_names);

        names
    }

    pub fn generator_names(&self) -> Vec<String> {
        self.inner
            .tx_generators
            .iter()
            .map(|(_, v)| v.name.clone())
            .collect()
    }

    pub fn generators_in_stream(&self, stream_scope: &StreamScopeRef) -> Vec<TransactionStreamRef> {
        match stream_scope {
            StreamScopeRef::Root => self
                .inner
                .tx_streams
                .iter()
                .map(|(_, v)| TransactionStreamRef {
                    gen_id: None,
                    stream_id: v.id,
                    name: v.name.clone(),
                })
                .collect(),
            StreamScopeRef::Stream(stream) => self
                .inner
                .tx_streams
                .get(&stream.stream_id)
                .unwrap()
                .generators
                .iter()
                .map(|id| {
                    let gen = self.inner.tx_generators.get(id).unwrap();
                    TransactionStreamRef {
                        gen_id: Some(gen.id),
                        stream_id: *id,
                        name: gen.name.clone(),
                    }
                })
                .collect(),
        }
    }

    pub fn max_timestamp(&self) -> Option<BigUint> {
        Some(BigUint::try_from(&self.inner.max_timestamp).unwrap())
    }

    pub fn metadata(&self) -> MetaData {
        let timescale = self.inner.time_scale;
        MetaData {
            date: None,
            version: None,
            timescale: TimeScale {
                unit: TimeUnit::from(timescale),
                multiplier: None,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StreamScopeRef {
    Root,
    Stream(TransactionStreamRef),
}

impl Display for StreamScopeRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamScopeRef::Root => write!(f, "Root scope"),
            StreamScopeRef::Stream(s) => s.fmt(f),
        }
    }
}

/// If `gen_id` is `Some` this `TransactionStreamRef` is a generator, otherwise it's a stream
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TransactionStreamRef {
    pub stream_id: usize,
    pub gen_id: Option<usize>,
    pub name: String,
}

impl Display for TransactionStreamRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        if self.is_generator() {
            write!(
                f,
                "Generator: id: {}, stream_id: {}, name: {}",
                self.gen_id.unwrap(),
                self.stream_id,
                self.name
            )
        } else {
            write!(f, "Stream: id: {}, name: {}", self.stream_id, self.name)
        }
    }
}

impl TransactionStreamRef {
    pub fn new_stream(stream_id: usize, name: String) -> Self {
        TransactionStreamRef {
            stream_id,
            gen_id: None,
            name,
        }
    }
    pub fn new_gen(stream_id: usize, gen_id: usize, name: String) -> Self {
        TransactionStreamRef {
            stream_id,
            gen_id: Some(gen_id),
            name,
        }
    }

    pub fn is_generator(&self) -> bool {
        self.gen_id.is_some()
    }

    pub fn is_stream(&self) -> bool {
        self.is_generator().not()
    }
}

pub struct TransactionRef {
    id: usize,
}
