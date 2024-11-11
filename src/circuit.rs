use crate::gates::TwoQubitGate;
use std::fmt;
use std::string::String;
use std::vec::Vec;

/// Circuit represented as a sequence of basic two-qubit gates
pub struct Circuit {
    gates: Vec<TwoQubitGate>,
    stages: Vec<Vec<usize>>,
    n_qubits: u32,
}

impl Circuit {
    pub fn new() -> Circuit {
        Circuit {
            gates: Vec::new(),
            stages: Vec::new(),
            n_qubits: 0,
        }
    }

    pub fn add(&mut self, g: TwoQubitGate) {
        self.n_qubits = self.n_qubits.max(g.q_ctrl);
        self.n_qubits = self.n_qubits.max(g.q_target);
        self.gates.push(g);
        self.stages.push(vec![self.gates.len() - 1]);
    }
}

impl fmt::Display for Circuit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let gates_rep = self.gates.iter().fold(String::new(), |s, g| {
            let delim = if s.is_empty() { "" } else { ", " };
            s + delim + &format!("{}", &g)
        });

        write!(f, "Circuit with {} gates:\n    {}", self.gates.len(), gates_rep)
    }
}
