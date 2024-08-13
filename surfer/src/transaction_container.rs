use crate::time::{TimeScale, TimeUnit};
use crate::wave_container::MetaData;
use ftr_parser::types::{TxGenerator, TxStream, FTR};
use itertools::Itertools;
use num::BigUint;
use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::ops::Not;

pub struct TransactionContainer {
    pub inner: FTR,
}

impl TransactionContainer {
    pub fn get_streams(&self) -> Vec<&TxStream> {
        self.inner.tx_streams.values().collect()
    }

    pub fn get_stream(&self, stream_id: usize) -> Option<&TxStream> {
        self.inner.get_stream(stream_id)
    }

    pub fn get_stream_from_name(&self, name: String) -> Option<&TxStream> {
        self.inner.get_stream_from_name(name)
    }

    pub fn get_generators(&self) -> Vec<&TxGenerator> {
        self.inner.tx_generators.values().collect()
    }

    pub fn get_generator(&self, gen_id: usize) -> Option<&TxGenerator> {
        self.inner.get_generator(gen_id)
    }
    pub fn get_generator_from_name(
        &self,
        stream_id: Option<usize>,
        gen_name: String,
    ) -> Option<&TxGenerator> {
        self.inner.get_generator_from_name(stream_id, gen_name)
    }

    pub fn get_transactions_from_generator(&self, gen_id: usize) -> Vec<usize> {
        self.get_generator(gen_id)
            .unwrap()
            .transactions
            .iter()
            .map(|t| t.get_tx_id())
            .collect_vec()
    }

    pub fn get_transactions_from_stream(&self, stream_id: usize) -> Vec<usize> {
        self.get_stream(stream_id)
            .unwrap()
            .generators
            .iter()
            .map(|g| {
                self.get_generator(*g)
                    .unwrap()
                    .transactions
                    .iter()
                    .map(|t| t.get_tx_id())
                    .collect_vec()
            })
            .flatten()
            .collect()
    }
    pub fn stream_scope_exists(&self, stream_scope: &StreamScopeRef) -> bool {
        match stream_scope {
            StreamScopeRef::Root => true,
            StreamScopeRef::Stream(s) => self.inner.tx_streams.contains_key(&s.stream_id),
            StreamScopeRef::Empty(_) => false,
        }
    }

    pub fn stream_names(&self) -> Vec<String> {
        let mut names = vec![String::from("tr")];
        let mut stream_names: Vec<String> = self
            .get_streams()
            .into_iter()
            .map(|s| s.name.clone())
            .collect();
        names.append(&mut stream_names);

        names
    }

    pub fn generator_names(&self) -> Vec<String> {
        self.get_generators()
            .into_iter()
            .map(|g| g.name.clone())
            .collect()
    }

    pub fn generators_in_stream(&self, stream_scope: &StreamScopeRef) -> Vec<TransactionStreamRef> {
        match stream_scope {
            StreamScopeRef::Root => self
                .get_streams()
                .into_iter()
                .map(|s| TransactionStreamRef {
                    gen_id: None,
                    stream_id: s.id,
                    name: s.name.clone(),
                })
                .collect(),
            StreamScopeRef::Stream(stream_ref) => self
                .get_stream(stream_ref.stream_id)
                .unwrap()
                .generators
                .iter()
                .map(|id| {
                    let gen = self.get_generator(*id).unwrap();
                    TransactionStreamRef {
                        gen_id: Some(gen.id),
                        stream_id: stream_ref.stream_id,
                        name: gen.name.clone(),
                    }
                })
                .collect(),
            StreamScopeRef::Empty(_) => vec![],
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

    pub fn body_loaded(&self) -> bool {
        true // for now
    }

    pub fn is_fully_loaded(&self) -> bool {
        true // for now
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StreamScopeRef {
    Root,
    Stream(TransactionStreamRef),
    Empty(String),
}

impl Display for StreamScopeRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            StreamScopeRef::Root => write!(f, "Root scope"),
            StreamScopeRef::Stream(s) => s.fmt(f),
            StreamScopeRef::Empty(_) => write!(f, "Empty stream scope"),
        }
    }
}

impl StreamScopeRef {
    pub fn new_stream_from_name(transactions: &TransactionContainer, name: String) -> Self {
        let stream = transactions
            .inner
            .get_stream_from_name(name.clone())
            .unwrap();
        StreamScopeRef::Stream(TransactionStreamRef::new_stream(stream.id, name))
    }
}

/// If `gen_id` is `Some` this `TransactionStreamRef` is a generator, otherwise it's a stream
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionStreamRef {
    pub stream_id: usize,
    pub gen_id: Option<usize>,
    pub name: String,
}

impl Hash for TransactionStreamRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.gen_id.unwrap_or(self.stream_id).hash(state);
        self.name.hash(state);
    }
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

#[derive(Clone, Debug, Eq, Hash, Serialize, Deserialize, PartialEq)]
pub struct TransactionRef {
    pub id: usize,
}
