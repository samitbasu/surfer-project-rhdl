use itertools::Itertools;
use std::str::FromStr;

use serde::Deserialize;

use crate::{displayed_item::DisplayedItem, wave_data::WaveData};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize)]
pub enum SignalNameType {
    Local,  // local signal name only (i.e. for tb.dut.clk => clk)
    Unique, // add unique prefix, prefix + local
    Global, // full signal name (i.e. tb.dut.clk => tb.dut.clk)
}

impl FromStr for SignalNameType {
    type Err = String;

    fn from_str(input: &str) -> Result<SignalNameType, Self::Err> {
        match input {
            "Local" => Ok(SignalNameType::Local),
            "Unique" => Ok(SignalNameType::Unique),
            "Global" => Ok(SignalNameType::Global),
            _ => Err(format!(
                "'{}' is not a valid SignalNameType (Valid options: Local|Unique|Global)",
                input
            )),
        }
    }
}

impl std::fmt::Display for SignalNameType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SignalNameType::Local => write!(f, "Local"),
            SignalNameType::Unique => write!(f, "Unique"),
            SignalNameType::Global => write!(f, "Global"),
        }
    }
}

impl WaveData {
    pub fn compute_signal_display_names(&mut self) {
        let full_names = self
            .displayed_items
            .iter()
            .filter_map(|item| match item {
                DisplayedItem::Signal(signal_ref) => Some(signal_ref),
                _ => None,
            })
            .map(|sig| sig.signal_ref.full_path_string())
            .unique()
            .collect_vec();

        for item in &mut self.displayed_items {
            match item {
                DisplayedItem::Signal(signal) => {
                    let local_name = signal.signal_ref.name.clone();
                    signal.display_name = match signal.display_name_type {
                        SignalNameType::Local => local_name,
                        SignalNameType::Global => signal.signal_ref.full_path_string(),
                        SignalNameType::Unique => {
                            /// This function takes a full signal name and a list of other
                            /// full signal names and returns a minimal unique signal name.
                            /// It takes scopes from the back of the signal until the name is unique.
                            // FIXME: Rewrite this to take SignalRef which already has done the
                            // `.` splitting
                            fn unique(signal: String, signals: &[String]) -> String {
                                // if the full signal name is very short just return it
                                if signal.len() < 20 {
                                    return signal;
                                }

                                let split_this =
                                    signal.split('.').map(|p| p.to_string()).collect_vec();
                                let split_signals = signals
                                    .iter()
                                    .filter(|&s| *s != signal)
                                    .map(|s| s.split('.').map(|p| p.to_string()).collect_vec())
                                    .collect_vec();

                                fn take_front(s: &Vec<String>, l: usize) -> String {
                                    if l == 0 {
                                        s.last().unwrap().clone()
                                    } else if l < s.len() - 1 {
                                        format!("...{}", s.iter().rev().take(l + 1).rev().join("."))
                                    } else {
                                        s.join(".")
                                    }
                                }

                                let mut l = 0;
                                while split_signals
                                    .iter()
                                    .map(|s| take_front(s, l))
                                    .contains(&take_front(&split_this, l))
                                {
                                    l += 1;
                                }
                                take_front(&split_this, l)
                            }

                            let full_name = signal.signal_ref.full_path_string();
                            unique(full_name, &full_names)
                        }
                    };
                }
                DisplayedItem::Divider(_) => {}
                DisplayedItem::Cursor(_) => {}
            }
        }
    }
}

pub fn force_signal_name_type(waves: &mut WaveData, name_type: SignalNameType) {
    for signal in &mut waves.displayed_items {
        if let DisplayedItem::Signal(signal) = signal {
            signal.display_name_type = name_type;
        }
    }
    waves.default_signal_name_type = name_type;
    waves.compute_signal_display_names();
}
