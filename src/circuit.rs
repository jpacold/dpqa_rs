use crate::gates::TwoQubitGate;
use std::collections::HashSet;
use std::fmt;
use std::string::String;
use std::vec::Vec;

/// Circuit represented as a sequence of basic two-qubit gates
pub struct Circuit {
    gates: Vec<TwoQubitGate>,
    stages: Vec<Vec<usize>>,
    n_qubits: usize,
}

impl Circuit {
    pub fn new() -> Circuit {
        Circuit {
            gates: Vec::new(),
            stages: Vec::new(),
            n_qubits: 0,
        }
    }

    /// Append a two-qubit gate to the circuit
    pub fn append(&mut self, g: TwoQubitGate) {
        self.n_qubits = self.n_qubits.max(g.q_ctrl + 1);
        self.n_qubits = self.n_qubits.max(g.q_target + 1);
        self.gates.push(g);
        self.stages.push(vec![self.gates.len() - 1]);
    }

    /// Get the number of qubits needed by the gates in this circuit
    pub fn get_n_qubits(&self) -> usize {
        self.n_qubits
    }

    /// Re-number qubits so that the indices of all qubits used by `self.gates`
    /// are consecutive integers starting from 0. Returns `true` if any indices
    /// were changed.
    pub fn renumber_qubits(&mut self) -> bool {
        let mut seen = vec![false; self.n_qubits];
        for g in &self.gates {
            seen[g.q_ctrl] = true;
            seen[g.q_target] = true;
        }
        if seen.iter().all(|&x| x) {
            return false;
        }

        let mut new_idx = vec![0; self.n_qubits];
        let mut nn = 0;
        for (jj, &x) in seen.iter().enumerate() {
            if x {
                new_idx[jj] = nn;
                nn += 1;
            }
        }
        let renumbered_gates = self
            .gates
            .iter()
            .map(|g| TwoQubitGate::new(g.gate_type, new_idx[g.q_ctrl], new_idx[g.q_target]))
            .collect();
        self.gates = renumbered_gates;

        self.n_qubits = nn;

        true
    }

    /// Group gates into "stages", i.e. sets that act on different qubits
    /// (which can be executed in parallel). Returns true if any gates were
    /// moved into different stages.
    pub fn recalculate_stages(&mut self) -> bool {
        let mut new_stages: Vec<HashSet<usize>> = Vec::new();
        let mut qubits_used: Vec<Vec<bool>> = Vec::new();

        for (ii, g) in self.gates.iter().enumerate() {
            let n_s = new_stages.len();
            let mut stage_idx = n_s;

            for jj in (0..n_s).rev() {
                if !(qubits_used[jj][g.q_ctrl] || qubits_used[jj][g.q_target]) {
                    // We could add the gate to this stage
                    stage_idx = jj;
                }

                // Check whether we could push the gate back to the previous
                // stage. This is possible if it commutes with all the gates
                // in the current stage.
                let commutes = new_stages[jj]
                    .iter()
                    .all(|&gate_idx| self.gates[gate_idx].commutes_with(g));
                if !commutes {
                    break;
                }
            }

            if stage_idx == n_s {
                new_stages.push(HashSet::new());
                qubits_used.push(vec![false; self.n_qubits]);
            }
            new_stages[stage_idx].insert(ii);
            qubits_used[stage_idx][g.q_ctrl] = true;
            qubits_used[stage_idx][g.q_target] = true;
        }

        let tmp = new_stages
            .into_iter()
            .map(|s| s.into_iter().collect())
            .collect();

        if tmp == self.stages {
            return false;
        }
        self.stages = tmp;
        true
    }

    pub fn get_n_stages(&self) -> usize {
        self.stages.len()
    }
}

impl fmt::Display for Circuit {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let gates_rep = self.gates.iter().fold(String::new(), |s, g| {
            let delim = if s.is_empty() { "" } else { ", " };
            s + delim + &format!("{}", &g)
        });

        write!(
            f,
            "Circuit with {} gates:\n    {}",
            self.gates.len(),
            gates_rep
        )
    }
}

#[cfg(test)]
mod tests {
    use super::Circuit;
    use super::TwoQubitGate;
    use crate::gates::TwoQubitGateType::{CX, CZ};

    #[test]
    fn one_gate() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 0, 1));
        assert_eq!(circuit.get_n_qubits(), 2);
    }

    #[test]
    fn two_gates() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 0, 1));
        circuit.append(TwoQubitGate::new(CZ, 0, 1));
        assert_eq!(circuit.get_n_qubits(), 2);
    }

    #[test]
    fn renumber() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 1, 2));
        circuit.append(TwoQubitGate::new(CZ, 2, 5));
        assert_eq!(circuit.get_n_qubits(), 6);
        assert!(circuit.renumber_qubits());
        assert_eq!(circuit.get_n_qubits(), 3);
    }

    #[test]
    fn restage_1() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 0, 1));
        circuit.append(TwoQubitGate::new(CZ, 2, 3));
        assert_eq!(circuit.get_n_stages(), 2);
        assert!(circuit.recalculate_stages());
        assert_eq!(circuit.get_n_stages(), 1);
    }

    #[test]
    fn restage_2() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 0, 1));
        circuit.append(TwoQubitGate::new(CX, 1, 2));
        circuit.append(TwoQubitGate::new(CX, 3, 2));
        circuit.append(TwoQubitGate::new(CX, 2, 3));
        assert_eq!(circuit.get_n_stages(), 4);
        assert!(circuit.recalculate_stages());
        assert_eq!(circuit.get_n_stages(), 3);
    }
}
