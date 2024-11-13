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

    pub fn solve(&self, circuit: &Circuit) -> bool {
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
        assert!(dpqa.solve(&circuit))
    }
}
