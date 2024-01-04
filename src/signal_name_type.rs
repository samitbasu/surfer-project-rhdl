use enum_iterator::Sequence;
use itertools::Itertools;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::{displayed_item::DisplayedItem, wave_container::SignalRef, wave_data::WaveData};

#[derive(PartialEq, Copy, Clone, Debug, Deserialize, Serialize, Sequence)]
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
                DisplayedItem::Signal(signal) => Some(signal.signal_ref.clone()),
                _ => None,
            })
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
                            fn unique(signal: &SignalRef, signals: &[SignalRef]) -> String {
                                let other_signals = signals
                                    .iter()
                                    .filter(|&s| *s.full_path_string() != signal.full_path_string())
                                    .collect_vec();

                                fn take_front(s: &SignalRef, l: usize) -> String {
                                    if l == 0 {
                                        s.name.clone()
                                    } else {
                                        format!(
                                            "{}{}.{}",
                                            if l < s.path.0.len() { "…" } else { "" },
                                            s.path.0.iter().rev().take(l).rev().join("."),
                                            s.name
                                        )
                                    }
                                }

                                let mut l = 0;
                                while other_signals
                                    .iter()
                                    .map(|s| take_front(s, l))
                                    .contains(&take_front(signal, l))
                                {
                                    l += 1;
                                }
                                take_front(signal, l)
                            }

                            unique(&signal.signal_ref, &full_names)
                        }
                    };
                }
                DisplayedItem::Divider(_) => {}
                DisplayedItem::Cursor(_) => {}
                DisplayedItem::TimeLine(_) => {}
            }
        }
    }

    pub fn force_signal_name_type(&mut self, name_type: SignalNameType) {
        for signal in &mut self.displayed_items {
            if let DisplayedItem::Signal(signal) = signal {
                signal.display_name_type = name_type;
            }
        }
        self.default_signal_name_type = name_type;
        self.compute_signal_display_names();
    }
}
