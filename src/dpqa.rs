use crate::{circuit::Circuit, variables::DPQAVars};
use std::fmt;
use z3::{Config, Context, SatResult, Solver};

/// DPQA solver
pub struct DPQA {
    rows: u64,
    cols: u64,
    aod_rows: u64,
    aod_cols: u64,
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
        }
    }

    // ToDo: this should return a result struct with a list of operations,
    // instead of just a bool
    pub fn solve(&self, circuit: &Circuit, extra_stages: Option<usize>) -> bool {
        let cfg = Config::new();
        let ctx = Context::new(&cfg);
        let solver = Solver::new(&ctx);

        let vars = DPQAVars::new(
            &ctx,
            circuit,
            self.rows,
            self.cols,
            self.aod_rows,
            self.aod_cols,
            circuit.get_n_stages() + extra_stages.unwrap_or(0),
        );
        vars.set_constraints(&solver);

        solver.check() == SatResult::Sat
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
    use super::DPQA;
    use crate::circuit::Circuit;
    use crate::gates::TwoQubitGate;
    use crate::gates::TwoQubitGateType::CX;

    #[test]
    fn one_gate() {
        let mut circuit = Circuit::new();
        circuit.append(TwoQubitGate::new(CX, 0, 1));

        let dpqa = DPQA::new(2, 1);
        assert!(dpqa.solve(&circuit, None));
    }

    #[test]
    // Circuit from Fig.2 of the OLSQ-DPQA paper
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
            circuit.append(TwoQubitGate::new(CX, p.0, p.1));
        }
        assert!(!circuit.renumber_qubits());
        assert!(circuit.recalculate_stages());
        assert_eq!(circuit.get_n_stages(), 4);

        let dpqa = DPQA::new(2, 4);
        assert!(dpqa.solve(&circuit, None));
    }
}
