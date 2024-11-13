use crate::circuit::Circuit;
use z3::{
    ast::{self, Ast},
    Context, Solver,
};

/// Variables used by the DPQA solver
pub struct DPQAVars<'ctx> {
    n_qubits: usize,
    n_stages: usize,

    // Grid bounds
    zero: ast::Int<'ctx>,
    x_max: ast::Int<'ctx>,
    y_max: ast::Int<'ctx>,
    c_max: ast::Int<'ctx>,
    r_max: ast::Int<'ctx>,

    /// Qubit positions
    x: Vec<Vec<ast::Int<'ctx>>>,
    y: Vec<Vec<ast::Int<'ctx>>>,
    c: Vec<Vec<ast::Int<'ctx>>>,
    r: Vec<Vec<ast::Int<'ctx>>>,

    /// Determines whether qubit is in SLM (false) or AOD (true)
    in_aod: Vec<Vec<ast::Bool<'ctx>>>,
}

impl<'ctx> DPQAVars<'ctx> {
    fn qubit_int_vars<'c>(
        context: &'c Context,
        n_qubits: usize,
        n_stages: usize,
        var_name: &str,
    ) -> Vec<Vec<ast::Int<'c>>> {
        (0..n_qubits)
            .map(|ii| {
                (0..n_stages)
                    .map(|jj| ast::Int::new_const(context, format!("{}_q{}_t{}", var_name, ii, jj)))
                    .collect()
            })
            .collect()
    }

    fn qubit_bool_vars<'c>(
        context: &'c Context,
        n_qubits: usize,
        n_stages: usize,
        var_name: &str,
    ) -> Vec<Vec<ast::Bool<'c>>> {
        (0..n_qubits)
            .map(|ii| {
                (0..n_stages)
                    .map(|jj| {
                        ast::Bool::new_const(context, format!("{}_q{}_t{}", var_name, ii, jj))
                    })
                    .collect()
            })
            .collect()
    }

    pub fn new(
        context: &'ctx Context,
        circuit: &Circuit,
        rows: u64,
        cols: u64,
        aod_rows: u64,
        aod_cols: u64,
    ) -> DPQAVars<'ctx> {
        let n_qubits = circuit.get_n_qubits();
        let n_stages = circuit.get_n_stages();

        DPQAVars {
            n_qubits: n_qubits,
            n_stages: n_stages,
            zero: ast::Int::from_u64(&context, 0),
            x_max: ast::Int::from_u64(&context, cols),
            y_max: ast::Int::from_u64(&context, rows),
            c_max: ast::Int::from_u64(&context, aod_cols),
            r_max: ast::Int::from_u64(&context, aod_rows),
            x: DPQAVars::qubit_int_vars(&context, n_qubits, n_stages, "x"),
            y: DPQAVars::qubit_int_vars(&context, n_qubits, n_stages, "y"),
            c: DPQAVars::qubit_int_vars(&context, n_qubits, n_stages, "c"),
            r: DPQAVars::qubit_int_vars(&context, n_qubits, n_stages, "r"),
            in_aod: DPQAVars::qubit_bool_vars(&context, n_qubits, n_stages, "a"),
        }
    }

    fn constrain_2d_vec<'c>(
        solver: &Solver,
        vars: &Vec<Vec<ast::Int<'c>>>,
        lower_bound: &ast::Int<'c>,
        upper_bound: &ast::Int<'c>,
    ) {
        for var_vec in vars {
            for var in var_vec {
                let lb = var.ge(&lower_bound);
                solver.assert(&lb);
                let ub = var.lt(&upper_bound);
                solver.assert(&ub);
            }
        }
    }

    /// Constrain all qubits to stay within grid bounds
    fn constraint_grid_bounds(&self, solver: &Solver) {
        DPQAVars::constrain_2d_vec(solver, &self.x, &self.zero, &self.x_max);
        DPQAVars::constrain_2d_vec(solver, &self.y, &self.zero, &self.y_max);
        DPQAVars::constrain_2d_vec(solver, &self.c, &self.zero, &self.c_max);
        DPQAVars::constrain_2d_vec(solver, &self.r, &self.zero, &self.r_max);
    }

    /// Any qubit in an SLM trap must stay in SLM between stages
    fn constraint_fixed_slm(&self, solver: &Solver) {
        for ii in 0..self.n_qubits {
            for jj in 1..self.n_stages {
                let x_fixed = self.x[ii][jj - 1]._eq(&self.x[ii][jj]);
                let x_slm = self.in_aod[ii][jj].not().implies(&x_fixed);
                solver.assert(&x_slm);

                let y_fixed = self.y[ii][jj - 1]._eq(&self.y[ii][jj]);
                let y_slm = self.in_aod[ii][jj].not().implies(&y_fixed);
                solver.assert(&y_slm);
            }
        }
    }

    /// Rows and columns of the AOD grid must move together
    fn constraint_aod_move_together(&self, solver: &Solver) {
        for ii in 0..self.n_qubits {
            for jj in 1..self.n_stages {
                let c_fixed = self.c[ii][jj - 1]._eq(&self.c[ii][jj]);
                let c_aod = self.in_aod[ii][jj].implies(&c_fixed);
                solver.assert(&c_aod);

                let r_fixed = self.r[ii][jj - 1]._eq(&self.r[ii][jj]);
                let r_slm = self.in_aod[ii][jj].implies(&r_fixed);
                solver.assert(&r_slm);
            }
        }

        // If any two qubits are in the same AOD row, and the AOD row moves,
        // then the two qubits must end up in the same row of the grid (i.e.
        // at the same value of y), and similarly for columns.
        let context = solver.get_context();
        for ii_0 in 0..self.n_qubits {
            for ii_1 in 0..self.n_qubits {
                for jj in 1..self.n_stages {
                    let both_aod =
                        ast::Bool::and(context, &[&self.in_aod[ii_0][jj], &self.in_aod[ii_1][jj]]);

                    let start_col_eq = self.c[ii_0][jj - 1]._eq(&self.c[ii_1][jj - 1]);
                    let move_col_together = ast::Bool::and(context, &[&both_aod, &start_col_eq]);
                    let next_col_eq = self.c[ii_0][jj]._eq(&self.c[ii_1][jj]);
                    solver.assert(&move_col_together.implies(&next_col_eq));

                    let start_row_eq = self.r[ii_0][jj - 1]._eq(&self.r[ii_1][jj - 1]);
                    let move_row_together = ast::Bool::and(context, &[&both_aod, &start_row_eq]);
                    let next_row_eq = self.r[ii_0][jj]._eq(&self.r[ii_1][jj]);
                    solver.assert(&move_row_together.implies(&next_row_eq));
                }
            }
        }
    }

    /// Set all constraints
    pub fn set_constraints(&self, solver: &Solver) {
        self.constraint_grid_bounds(solver);
        self.constraint_fixed_slm(solver);
        self.constraint_aod_move_together(solver);
    }
}
