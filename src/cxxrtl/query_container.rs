use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use base64::prelude::*;
use futures::executor::block_on;
use num::{bigint::ToBigInt, BigInt, BigUint};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use tokio::{
    sync::RwLock,
    task::{spawn_blocking, JoinHandle},
};

use crate::{
    cxxrtl_container::CxxrtlItem,
    message::Message,
    wave_container::{QueryResult, VariableRef, VariableValue},
};

use super::sc_message::CxxrtlSample;

type ValueList = Arc<RwLock<BTreeMap<BigInt, HashMap<VariableRef, VariableValue>>>>;

pub struct QueryContainer {
    signal_values: ValueList,
    worker_handle: Option<JoinHandle<()>>,
}

impl QueryContainer {
    pub fn empty() -> Self {
        QueryContainer {
            signal_values: Arc::new(RwLock::new(BTreeMap::new())),
            worker_handle: None,
        }
    }

    pub fn invalidate(&mut self) {
        if let Some(wh) = &self.worker_handle {
            wh.abort();
            self.worker_handle = None
        }
        self.signal_values = Arc::new(RwLock::new(BTreeMap::new()))
    }

    pub fn populate(
        &mut self,
        signals: Vec<VariableRef>,
        item_info: Arc<HashMap<VariableRef, CxxrtlItem>>,
        data: Vec<CxxrtlSample>,
        msg_sender: std::sync::mpsc::Sender<Message>,
    ) {
        let signal_values = self.signal_values.clone();

        let wh = tokio::spawn(async {
            fill_signal_values(signals, item_info, data, signal_values, msg_sender).await
        });

        self.worker_handle = Some(wh);
    }

    pub fn query(&self, var: &VariableRef, query_time: BigInt) -> QueryResult {
        let values = block_on(self.signal_values.read());

        if let Some((time, value_map)) = values.range(..query_time.clone()).next_back() {
            match (time.to_biguint(), value_map.get(var)) {
                (Some(time), Some(value)) => {
                    let next = values
                        .range(query_time..)
                        .next()
                        .and_then(|(k, _)| k.to_biguint());
                    QueryResult {
                        current: Some((time.clone(), value.clone())),
                        next,
                    }
                }
                _ => QueryResult::default(),
            }
        } else {
            QueryResult::default()
        }
    }
}

async fn fill_signal_values(
    variables: Vec<VariableRef>,
    item_info: Arc<HashMap<VariableRef, CxxrtlItem>>,
    data: Vec<CxxrtlSample>,
    signal_values: ValueList,
    msg_sender: std::sync::mpsc::Sender<Message>,
) {
    // Since this is a purely CPU bound operation, we'll spawn a blocking task to
    // perform it
    spawn_blocking(move || {
        // Once we base64 decode the cxxrtl data, we'll end up with a bunch of u32s, where
        // the signals are packed next to each other. We'll start off computing the offset
        // of each signal for later use
        let mut offset = 0;
        let mut ranges = vec![];
        for variable in &variables {
            let this_size_bits = &item_info[variable].width;
            let this_size_u32 = 1 + ((this_size_bits - 1) / 32);
            ranges.push((offset * 4) as usize..((offset + this_size_u32) * 4) as usize);
            offset += this_size_u32
        }

        data.par_iter().for_each(|sample| {
            let u8s = BASE64_STANDARD
                .decode(&sample.item_values)
                .map_err(|e| {
                    panic!(
                        "Got non-base64 data from cxxrtl at time {}. {e}",
                        sample.time
                    )
                })
                .unwrap();

            let values = ranges
                .iter()
                .zip(&variables)
                .map(|(range, var)| {
                    let value = BigUint::from_bytes_le(&u8s[range.clone()]);

                    // TODO: Probably shouldn't have this indexed by the signal ref here so we can
                    // avoid the clone
                    (var.clone(), VariableValue::BigUint(value))
                })
                .collect::<HashMap<_, _>>();

            block_on(signal_values.write())
                .insert(sample.time.into_femtoseconds().to_bigint().unwrap(), values);
            msg_sender
                .send(Message::InvalidateDrawCommands)
                .expect("Message receiver disconnected");
        })
    });
}
