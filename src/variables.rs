use crate::circuit::Circuit;
use std::collections::HashMap;
use z3::{
    ast::{self, Ast},
    Context, Solver,
};

/// Variables used by the DPQA solver
pub struct DPQAVars<'ctx, 'circ> {
    circuit: &'circ Circuit,
    n_qubits: usize,
    n_stages: usize,

    // Grid bounds
    zero: ast::Int<'ctx>,
    x_max: ast::Int<'ctx>,
    y_max: ast::Int<'ctx>,
    c_max: ast::Int<'ctx>,
    r_max: ast::Int<'ctx>,

    // Qubit positions
    x: Vec<Vec<ast::Int<'ctx>>>,
    y: Vec<Vec<ast::Int<'ctx>>>,
    c: Vec<Vec<ast::Int<'ctx>>>,
    r: Vec<Vec<ast::Int<'ctx>>>,

    // Determines whether qubit is in SLM (false) or AOD (true)
    in_aod: Vec<Vec<ast::Bool<'ctx>>>,

    // Time when each gate is executed
    t: Vec<ast::Int<'ctx>>,
    t_max: ast::Int<'ctx>,
    t_order: Vec<(usize, usize)>,
    s_vals: Vec<ast::Int<'ctx>>,
}

/// Results from a successful solver run
pub struct DPQAVarsValues {
    pub xy: Vec<Vec<(u64, u64)>>,
    pub cr: Vec<Vec<(u64, u64)>>,
    pub aod: Vec<Vec<bool>>,
    pub t: Vec<u64>,
}

impl<'ctx, 'circ> DPQAVars<'ctx, 'circ> {
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
        circuit: &'circ Circuit,
        rows: u64,
        cols: u64,
        aod_rows: u64,
        aod_cols: u64,
        n_stages: usize,
    ) -> DPQAVars<'ctx, 'circ> {
        let n_qubits = circuit.get_n_qubits();
        let n_gates = circuit.get_n_two_qubit_gates();

        DPQAVars {
            circuit: circuit,
            n_qubits: circuit.get_n_qubits(),
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
            t: (0..n_gates)
                .map(|ii| ast::Int::new_const(context, format!("t_{}", ii)))
                .collect(),
            t_max: ast::Int::from_u64(&context, n_stages as u64),
            t_order: circuit.get_gate_ordering(),
            s_vals: (0..n_stages)
                .map(|ii| ast::Int::from_u64(&context, ii as u64))
                .collect(),
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

    /// The order of SLM columns must be consistent with the order
    /// of AOD columns
    fn constraint_slm_order_from_aod(&self, solver: &Solver) {
        let context = solver.get_context();
        for ii_0 in 0..self.n_qubits {
            for ii_1 in 0..self.n_qubits {
                if ii_1 == ii_0 {
                    continue;
                }
                for jj in 0..self.n_stages {
                    let both_aod =
                        ast::Bool::and(context, &[&self.in_aod[ii_0][jj], &self.in_aod[ii_1][jj]]);

                    let start_col_lt = self.x[ii_0][jj].lt(&self.x[ii_1][jj]);
                    let slm_col_order = ast::Bool::and(context, &[&both_aod, &start_col_lt]);
                    let aod_col_order = self.c[ii_0][jj].lt(&self.c[ii_1][jj]);
                    solver.assert(&slm_col_order.implies(&aod_col_order));

                    let start_row_lt = self.y[ii_0][jj].lt(&self.y[ii_1][jj]);
                    let slm_row_order = ast::Bool::and(context, &[&both_aod, &start_row_lt]);
                    let aod_row_order = self.r[ii_0][jj].lt(&self.r[ii_1][jj]);
                    solver.assert(&slm_row_order.implies(&aod_row_order));
                }
            }
        }
    }

    /// No crossing between AOD rows/columns
    fn constraint_aod_order_from_slm(&self, solver: &Solver) {
        let context = solver.get_context();
        for ii_0 in 0..self.n_qubits {
            for ii_1 in 0..self.n_qubits {
                if ii_1 == ii_0 {
                    continue;
                }
                for jj in 1..self.n_stages {
                    let both_aod = ast::Bool::and(
                        context,
                        &[&self.in_aod[ii_0][jj - 1], &self.in_aod[ii_1][jj - 1]],
                    );

                    let start_col_lt = self.c[ii_0][jj - 1].lt(&self.c[ii_1][jj - 1]);
                    let aod_col_order = ast::Bool::and(context, &[&both_aod, &start_col_lt]);
                    let slm_col_order = self.x[ii_0][jj].le(&self.x[ii_1][jj]);
                    solver.assert(&aod_col_order.implies(&slm_col_order));

                    let start_row_lt = self.r[ii_0][jj - 1].lt(&self.r[ii_1][jj - 1]);
                    let aod_row_order = ast::Bool::and(context, &[&both_aod, &start_row_lt]);
                    let slm_row_order = self.y[ii_0][jj].le(&self.y[ii_1][jj]);
                    solver.assert(&aod_row_order.implies(&slm_row_order));
                }
            }
        }
    }

    /// Prevent stacking/crowding of more than 3 AOD rows/columns
    fn constraint_aod_crowding(&self, solver: &Solver) {
        let context = solver.get_context();
        let max_stack = ast::Int::from_u64(&context, 3);

        for ii_0 in 0..self.n_qubits {
            for ii_1 in 0..self.n_qubits {
                if ii_1 == ii_0 {
                    continue;
                }
                for jj_1 in 0..self.n_stages {
                    let jj_0 = if jj_1 == 0 { 0 } else { jj_1 - 1 };

                    let both_aod = ast::Bool::and(
                        context,
                        &[&self.in_aod[ii_0][jj_0], &self.in_aod[ii_1][jj_0]],
                    );

                    let col_diff =
                        ast::Int::sub(&context, &[&self.c[ii_0][jj_0], &self.c[ii_1][jj_0]]);
                    let col_diff_bound =
                        ast::Bool::and(&context, &[&both_aod, &col_diff.ge(&max_stack)]);
                    solver.assert(
                        &col_diff_bound.implies(&self.x[ii_0][jj_1].gt(&self.x[ii_1][jj_1])),
                    );

                    let row_diff =
                        ast::Int::sub(&context, &[&self.r[ii_0][jj_0], &self.r[ii_1][jj_0]]);
                    let row_diff_bound =
                        ast::Bool::and(&context, &[&both_aod, &row_diff.ge(&max_stack)]);
                    solver.assert(
                        &row_diff_bound.implies(&self.y[ii_0][jj_1].gt(&self.y[ii_1][jj_1])),
                    );
                }
            }
        }
    }

    /// Limit traps to one atom at a time
    fn constraint_site_crowding(&self, solver: &Solver) {
        let context = solver.get_context();

        for ii_0 in 0..self.n_qubits {
            for ii_1 in (ii_0 + 1)..self.n_qubits {
                for jj in 0..self.n_stages {
                    let both_aod =
                        ast::Bool::and(context, &[&self.in_aod[ii_0][jj], &self.in_aod[ii_1][jj]]);
                    let cr_diff = ast::Bool::or(
                        &context,
                        &[
                            &self.c[ii_0][jj]._eq(&self.c[ii_1][jj]).not(),
                            &self.r[ii_0][jj]._eq(&self.r[ii_1][jj]).not(),
                        ],
                    );
                    solver.assert(&both_aod.implies(&cr_diff));

                    let both_slm = ast::Bool::and(
                        context,
                        &[&self.in_aod[ii_0][jj].not(), &self.in_aod[ii_1][jj].not()],
                    );
                    let xy_diff = ast::Bool::and(
                        &context,
                        &[
                            &self.x[ii_0][jj]._eq(&self.x[ii_1][jj]),
                            &self.y[ii_0][jj]._eq(&self.y[ii_1][jj]),
                        ],
                    )
                    .not();
                    solver.assert(&both_slm.implies(&xy_diff));
                }
            }
        }
    }

    /// Only allow AOD-SLM transfer when there is one atom at a given site
    fn constraint_no_swap(&self, solver: &Solver) {
        let context = solver.get_context();

        for ii_0 in 0..self.n_qubits {
            for ii_1 in (ii_0 + 1)..self.n_qubits {
                for jj in 1..self.n_stages {
                    let same_site = ast::Bool::and(
                        &context,
                        &[
                            &self.x[ii_0][jj]._eq(&self.x[ii_1][jj]),
                            &self.y[ii_0][jj]._eq(&self.y[ii_1][jj]),
                        ],
                    );
                    let no_swap = ast::Bool::and(
                        &context,
                        &[
                            &self.in_aod[ii_0][jj]._eq(&self.in_aod[ii_0][jj - 1]),
                            &self.in_aod[ii_1][jj]._eq(&self.in_aod[ii_1][jj - 1]),
                        ],
                    );
                    solver.assert(&same_site.implies(&no_swap));
                }
            }
        }
    }

    /// Restrict each gate time to 0 <= t < self.n_stages, and ensure that
    /// gates with dependencies on each other are run in the right order
    pub fn constraint_t_bounds(&self, solver: &Solver) {
        for t_var in &self.t {
            solver.assert(&t_var.ge(&self.zero));
            solver.assert(&t_var.lt(&self.t_max));
        }

        for &(g0, g1) in &self.t_order {
            solver.assert(&self.t[g0].lt(&self.t[g1]));
        }
    }

    /// Two qubits must be at the same grid position when an entangling gate
    /// is run on them
    pub fn constraint_entangling_gates(&self, solver: &Solver) {
        let context = solver.get_context();
        for jj in 0..self.n_stages {
            for (ii, g) in self.circuit.iter().enumerate() {
                let (q0, q1) = (g.q_ctrl, g.q_target);
                let same_pos = ast::Bool::and(
                    &context,
                    &[
                        &self.x[q0][jj]._eq(&self.x[q1][jj]),
                        &self.y[q0][jj]._eq(&self.y[q1][jj]),
                    ],
                );
                solver.assert(&self.t[ii]._eq(&self.s_vals[jj]).implies(&same_pos));
            }
        }
    }

    /// Two qubits may only be at the same grid position if they are both
    /// used by a gate
    pub fn constraint_interaction_exactness(&self, solver: &Solver) {
        // Maps a pair of qubits q0, q1 (with q0 < q1) to the indices of the
        // gate(s) that act on q0 and q1
        let mut interactions: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (ii, g) in self.circuit.iter().enumerate() {
            let (q0, q1) = (g.q_ctrl.min(g.q_target), g.q_target.max(g.q_ctrl));
            interactions.entry((q0, q1)).or_default().push(ii);
        }

        let context = solver.get_context();

        for ii_0 in 0..self.n_qubits {
            for ii_1 in (ii_0 + 1)..self.n_qubits {
                if let Some(gate_indices) = interactions.get(&(ii_0, ii_1)) {
                    // This pair of qubits can interact, but only at stages
                    // where both are used in a gate
                    for jj in 0..self.n_stages {
                        let qubits_coincident = ast::Bool::and(
                            &context,
                            &[
                                &self.x[ii_0][jj]._eq(&self.x[ii_1][jj]),
                                &self.y[ii_0][jj]._eq(&self.y[ii_1][jj]),
                            ],
                        );

                        let or_args: Vec<ast::Bool> = gate_indices
                            .iter()
                            .map(|&gg| self.t[gg]._eq(&self.s_vals[jj]))
                            .collect();
                        let gate_condition = ast::Bool::or(
                            &context,
                            or_args
                                .iter()
                                .map(|v| v)
                                .collect::<Vec<&ast::Bool>>()
                                .as_slice(),
                        );

                        solver.assert(&qubits_coincident.implies(&gate_condition));
                    }
                } else {
                    // This pair of qubits cannot interact
                    for jj in 0..self.n_stages {
                        let qubits_not_coincident = ast::Bool::or(
                            &context,
                            &[
                                &self.x[ii_0][jj]._eq(&self.x[ii_1][jj]).not(),
                                &self.y[ii_0][jj]._eq(&self.y[ii_1][jj]).not(),
                            ],
                        );
                        solver.assert(&qubits_not_coincident);
                    }
                }
            }
        }
    }

    /// Set all constraints
    pub fn set_constraints(&self, solver: &Solver) {
        // Architecture constraints
        self.constraint_grid_bounds(solver);
        self.constraint_fixed_slm(solver);
        self.constraint_aod_move_together(solver);
        self.constraint_slm_order_from_aod(solver);
        self.constraint_aod_order_from_slm(solver);
        self.constraint_aod_crowding(solver);
        self.constraint_site_crowding(solver);
        self.constraint_no_swap(solver);

        // Circuit-dependent constraints
        self.constraint_t_bounds(solver);
        self.constraint_entangling_gates(solver);
        self.constraint_interaction_exactness(solver);
    }

    /// Get the qubit positions and gate execution times. Panics
    /// if solver state != Sat.
    pub fn eval(&self, solver: &Solver) -> DPQAVarsValues {
        let model = solver.get_model().unwrap();

        let get_u64 = |var: &ast::Int| -> u64 { model.eval(var, true).unwrap().as_u64().unwrap() };

        let xy_result = (0..self.n_qubits)
            .map(|ii| {
                (0..self.n_stages)
                    .map(|jj| (get_u64(&self.x[ii][jj]), get_u64(&self.y[ii][jj])))
                    .collect()
            })
            .collect();

        let cr_result = (0..self.n_qubits)
            .map(|ii| {
                (0..self.n_stages)
                    .map(|jj| (get_u64(&self.c[ii][jj]), get_u64(&self.r[ii][jj])))
                    .collect()
            })
            .collect();

        let aod_result = self
            .in_aod
            .iter()
            .map(|vars| {
                vars.iter()
                    .map(|v| model.eval(v, true).unwrap().as_bool().unwrap())
                    .collect()
            })
            .collect();

        let t_result = self.t.iter().map(|v| get_u64(v)).collect();

        DPQAVarsValues {
            xy: xy_result,
            cr: cr_result,
            aod: aod_result,
            t: t_result,
        }
    }
}
