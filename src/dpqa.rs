use crate::{circuit::Circuit, gates::TwoQubitGate, variables::DPQAVars};
use std::collections::HashMap;
use std::fmt;
use z3::{Config, Context, SatResult, Solver};

/// DPQA solver
pub struct DPQA {
    rows: u64,
    cols: u64,
    aod_rows: u64,
    aod_cols: u64,
    extra_stages: usize,
}

/// Qubit array instructions
#[derive(PartialEq, Eq, Debug)]
pub enum DPQAInstruction {
    Init {
        qubit: usize,
        x: u64,
        y: u64,
        in_aod: bool,
    },
    MoveAODRow {
        qubits: Vec<usize>,
        y_from: u64,
        y_to: u64,
    },
    MoveAODCol {
        qubits: Vec<usize>,
        x_from: u64,
        x_to: u64,
    },
    MoveToSLM(usize),
    Gate(Vec<TwoQubitGate>),
}

/// Compilation result object
#[derive(PartialEq, Eq, Debug)]
pub enum DPQAResult {
    Failed,
    Succeeded(Vec<DPQAInstruction>),
}

impl DPQA {
    /// Create a new DPQA solver by specifying the size of the grid.
    /// ```
    /// use dpqa_rs::dpqa::DPQA;
    /// let dpqa_compiler = DPQA::new(3, 2);
    /// println!("{}", dpqa_compiler);
    /// ```
    pub fn new(rows: u64, cols: u64) -> DPQA {
        DPQA {
            rows: rows,
            cols: cols,
            aod_rows: rows,
            aod_cols: cols,
            extra_stages: 0,
        }
    }

    /// Create a new DPQA solver by specifying the grid, potentially
    /// with a differently sized grid of AOD traps.
    pub fn new_aod(rows: u64, cols: u64, aod_rows: u64, aod_cols: u64) -> DPQA {
        DPQA {
            rows,
            cols,
            aod_rows,
            aod_cols,
            extra_stages: 0,
        }
    }

    /// Set up constraints for the given architecture and circuit, the attempt
    /// to solve
    pub fn solve(&self, circuit: &Circuit) -> DPQAResult {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);
        let n_stages = circuit.get_n_stages() + self.extra_stages;

        let vars = DPQAVars::new(
            &ctx,
            circuit,
            self.rows,
            self.cols,
            self.aod_rows,
            self.aod_cols,
            n_stages,
        );
        vars.set_constraints(&solver);

        if solver.check() == SatResult::Sat {
            let n_qubits = circuit.get_n_qubits();

            let vals = vars.eval(&solver);
            let mut instructions: Vec<DPQAInstruction> = Vec::new();
            let n_gates = circuit.get_n_two_qubit_gates();
            let mut gate_idx = 0;

            for jj in 0..n_stages {
                if jj == 0 {
                    // Get initial state
                    for (ii, xy) in vals.xy.iter().enumerate() {
                        instructions.push(DPQAInstruction::Init {
                            qubit: ii,
                            x: xy[0].0,
                            y: xy[0].1,
                            in_aod: vals.aod[ii][0],
                        });
                    }
                } else {
                    // Check for AOD to SLM moves
                    for ii in 0..n_qubits {
                        if !vals.aod[ii][jj] && vals.aod[ii][jj - 1] {
                            instructions.push(DPQAInstruction::MoveToSLM(ii));
                        }
                    }

                    // Check for AOD grid moves
                    let mut moves_x: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
                    for ii in 0..n_qubits {
                        let x_prev = vals.xy[ii][jj - 1].0;
                        let x_curr = vals.xy[ii][jj].0;
                        let c_prev = vals.cr[ii][jj - 1].0;
                        let c_curr = vals.cr[ii][jj].0;
                        if x_curr != x_prev && c_curr == c_prev {
                            moves_x.entry((x_prev, x_curr)).or_default().push(ii);
                        }
                    }
                    for (mv, qubits) in moves_x.iter() {
                        instructions.push(DPQAInstruction::MoveAODCol {
                            qubits: qubits.clone(),
                            x_from: mv.0,
                            x_to: mv.1,
                        });
                    }

                    let mut moves_y: HashMap<(u64, u64), Vec<usize>> = HashMap::new();
                    for ii in 0..n_qubits {
                        let y_prev = vals.xy[ii][jj - 1].1;
                        let y_curr = vals.xy[ii][jj].1;
                        let r_prev = vals.cr[ii][jj - 1].1;
                        let r_curr = vals.cr[ii][jj].1;
                        if y_curr != y_prev && r_curr == r_prev {
                            moves_y.entry((y_prev, y_curr)).or_default().push(ii);
                        }
                    }
                    for (mv, qubits) in moves_y.iter() {
                        instructions.push(DPQAInstruction::MoveAODRow {
                            qubits: qubits.clone(),
                            y_from: mv.0,
                            y_to: mv.1,
                        });
                    }
                }

                // Report gates
                let mut gates_run = Vec::new();
                while gate_idx < n_gates && vals.t[gate_idx] as usize == jj {
                    gates_run.push(circuit.get_gate(gate_idx));
                    gate_idx += 1;
                }
                if !gates_run.is_empty() {
                    instructions.push(DPQAInstruction::Gate(gates_run));
                }
            }
            return DPQAResult::Succeeded(instructions);
        }
        DPQAResult::Failed
    }

    /// Increase the number of stages (time steps) beyond the minimum number
    /// needed to execute all the gates in the circuit
    pub fn set_extra_stages(&mut self, extra_stages: usize) {
        self.extra_stages = extra_stages;
    }
}

impl fmt::Display for DPQAInstruction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let aod_str = |in_aod: &bool| -> &str {
            if *in_aod {
                "AOD"
            } else {
                "SLM"
            }
        };

        match self {
            DPQAInstruction::Init {
                qubit,
                x,
                y,
                in_aod,
            } => write!(
                f,
                "Initialize qubit {} at x={}, y={} ({})",
                qubit,
                x,
                y,
                aod_str(in_aod)
            ),
            DPQAInstruction::MoveAODRow {
                qubits,
                y_from,
                y_to,
            } => write!(
                f,
                "Move qubit row {:?} from y={} to y={}",
                qubits, y_from, y_to
            ),
            DPQAInstruction::MoveAODCol {
                qubits,
                x_from,
                x_to,
            } => write!(
                f,
                "Move qubit column {:?} from x={} to x={}",
                qubits, x_from, x_to
            ),
            DPQAInstruction::MoveToSLM(qubit) => write!(f, "Moved qubit {} to SLM", qubit),
            DPQAInstruction::Gate(qubit_pairs) => {
                write!(f, "Execute {:?}", qubit_pairs)
            }
        }
    }
}

impl fmt::Display for DPQA {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "DPQA solver\n    grid:     {} x {}\n    AOD grid: {} x {}",
            self.rows, self.cols, self.aod_rows, self.aod_cols
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{DPQAResult, DPQA};
    use crate::circuit::Circuit;
    use crate::gates::TwoQubitGate;
    use crate::gates::TwoQubitGateType::{CX, CZ};

    #[test]
    fn one_gate() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CZ, 0, 1));

        let dpqa = DPQA::new(2, 1);
        assert!(matches!(dpqa.solve(&circuit), DPQAResult::Succeeded(_)));
    }

    #[test]
    /// Circuit requiring one move
    fn two_gates() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CZ, 0, 1));
        circuit.append(TwoQubitGate::new(CZ, 1, 2));

        let dpqa = DPQA::new(2, 1);
        let result = dpqa.solve(&circuit);

        if let DPQAResult::Succeeded(instructions) = result {
            for x in &instructions {
                println!("{}", x);
            }
        } else {
            assert!(false)
        }
    }

    #[test]
    /// Circuit requiring one row/column move
    fn four_gates() {
        let mut circuit = Circuit::new();

        circuit.append(TwoQubitGate::new(CZ, 0, 2));
        circuit.append(TwoQubitGate::new(CZ, 1, 3));
        circuit.append(TwoQubitGate::new(CZ, 2, 4));
        circuit.append(TwoQubitGate::new(CZ, 3, 5));
        circuit.recalculate_stages();

        let dpqa = DPQA::new(2, 2);
        let result = dpqa.solve(&circuit);

        if let DPQAResult::Succeeded(instructions) = result {
            for x in &instructions {
                println!("{}", x);
            }
        } else {
            assert!(false)
        }
    }

    #[test]
    /// Circuit requiring two row/column moves
    fn six_gates() {
        let mut circuit = Circuit::new();

        circuit.append(TwoQubitGate::new(CZ, 0, 2));
        circuit.append(TwoQubitGate::new(CZ, 1, 3));
        circuit.append(TwoQubitGate::new(CZ, 0, 4));
        circuit.append(TwoQubitGate::new(CZ, 1, 5));
        circuit.append(TwoQubitGate::new(CZ, 0, 6));
        circuit.append(TwoQubitGate::new(CZ, 1, 7));
        circuit.recalculate_stages();

        let dpqa = DPQA::new(3, 2);
        let result = dpqa.solve(&circuit);

        if let DPQAResult::Succeeded(instructions) = result {
            for x in &instructions {
                println!("{}", x);
            }
        } else {
            assert!(false)
        }
    }

    #[test]
    /// Circuit from Fig.2 of the OLSQ-DPQA paper
    fn fig_2_circuit() {
        let mut circuit = Circuit::new();
        let qubit_pairs = [
            (2, 4),
            (3, 5),
            (0, 1),
            (2, 3),
            (4, 5),
            (0, 2),
            (1, 3),
            (0, 4),
            (1, 5),
        ];
        for p in qubit_pairs {
            circuit.append(TwoQubitGate::new(CZ, p.0, p.1));
        }
        assert!(!circuit.renumber_qubits());
        assert!(circuit.recalculate_stages());
        assert_eq!(circuit.get_n_stages(), 4);

        let dpqa = DPQA::new(2, 4);
        let result = dpqa.solve(&circuit);

        if let DPQAResult::Succeeded(instructions) = result {
            for x in &instructions {
                println!("{}", x);
            }
        } else {
            assert!(false)
        }
    }

    #[test]
    // Check that different gate types are separated into different stages
    fn gate_types_separated() {
        let mut circuit = Circuit::new();

        circuit.append(TwoQubitGate::new(CZ, 0, 2));
        circuit.append(TwoQubitGate::new(CZ, 1, 3));
        circuit.append(TwoQubitGate::new(CX, 4, 5));
        circuit.append(TwoQubitGate::new(CX, 6, 7));
        circuit.recalculate_stages();

        // We need at least 6 total sites so that we can keep qubits 4-7
        // separated when we run the gates on qubits 0-4, and vice versa.
        {
            let dpqa_too_small = DPQA::new(2, 2);
            let failed = dpqa_too_small.solve(&circuit);
            assert!(failed == DPQAResult::Failed);
        }

        let dpqa = DPQA::new(2, 3);
        let result = dpqa.solve(&circuit);

        if let DPQAResult::Succeeded(instructions) = result {
            for x in &instructions {
                println!("{}", x);
            }
        } else {
            assert!(false)
        }
    }
}
