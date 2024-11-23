use crate::circuit::Circuit;
use itertools::{izip, Itertools};
use std::collections::HashMap;
use z3::{
    ast::{self, Ast},
    Context, Optimize,
};

/// Variables associated with a qubit
struct QubitVars<'ctx> {
    // Positions
    pub x: Vec<ast::Int<'ctx>>,
    pub y: Vec<ast::Int<'ctx>>,
    pub c: Vec<ast::Int<'ctx>>,
    pub r: Vec<ast::Int<'ctx>>,

    // Determines whether qubit is in SLM (false) or AOD (true)
    pub aod: Vec<ast::Bool<'ctx>>,
}

/// Variables associated with a qubit at a given stage
struct QubitVarsStage<'ctx> {
    x: &'ctx ast::Int<'ctx>,
    y: &'ctx ast::Int<'ctx>,
    c: &'ctx ast::Int<'ctx>,
    r: &'ctx ast::Int<'ctx>,
    aod: &'ctx ast::Bool<'ctx>,
}

impl<'ctx> QubitVars<'ctx> {
    pub fn new(context: &'ctx Context, qubit_idx: usize, n_stages: usize) -> QubitVars<'ctx> {
        let create_int_vars = |var_name: &str| -> Vec<ast::Int<'ctx>> {
            (0..n_stages)
                .map(|jj| {
                    ast::Int::new_const(context, format!("{}_q{}_t{}", var_name, qubit_idx, jj))
                })
                .collect()
        };

        QubitVars {
            x: create_int_vars("x"),
            y: create_int_vars("y"),
            c: create_int_vars("c"),
            r: create_int_vars("r"),
            aod: (0..n_stages)
                .map(|jj| {
                    ast::Bool::new_const(context, format!("{}_q{}_t{}", "aod", qubit_idx, jj))
                })
                .collect(),
        }
    }

    fn iter(&self) -> impl Iterator<Item = QubitVarsStage<'_>> {
        izip!(&self.x, &self.y, &self.c, &self.r, &self.aod)
            .map(|(x, y, c, r, aod)| QubitVarsStage { x, y, c, r, aod })
    }
}

/// Variables used by the DPQA solver
pub struct DPQAVars<'ctx, 'circ> {
    circuit: &'circ Circuit,
    zero: ast::Int<'ctx>,
    one: ast::Int<'ctx>,

    // Grid bounds
    x_max: ast::Int<'ctx>,
    y_max: ast::Int<'ctx>,
    c_max: ast::Int<'ctx>,
    r_max: ast::Int<'ctx>,

    // Qubit variables
    qubits: Vec<QubitVars<'ctx>>,

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
            zero: ast::Int::from_u64(&context, 0),
            one: ast::Int::from_u64(&context, 1),
            x_max: ast::Int::from_u64(&context, cols),
            y_max: ast::Int::from_u64(&context, rows),
            c_max: ast::Int::from_u64(&context, aod_cols),
            r_max: ast::Int::from_u64(&context, aod_rows),
            qubits: (0..n_qubits)
                .map(|ii| QubitVars::new(&context, ii, n_stages))
                .collect(),
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

    /// Constrain all qubits to stay within grid bounds
    fn constraint_grid_bounds(&self, solver: &Optimize) {
        let set_bounds = |vars: &[ast::Int], lower_bound: &ast::Int, upper_bound: &ast::Int| {
            for v in vars {
                let lb = v.ge(&lower_bound);
                solver.assert(&lb);
                let ub = v.lt(&upper_bound);
                solver.assert(&ub);
            }
        };

        for q in &self.qubits {
            set_bounds(&q.x, &self.zero, &self.x_max);
            set_bounds(&q.y, &self.zero, &self.y_max);
            set_bounds(&q.c, &self.zero, &self.c_max);
            set_bounds(&q.r, &self.zero, &self.r_max);
        }
    }

    fn require_unchanged(solver: &Optimize, condition: &ast::Bool, var: &[ast::Int]) {
        solver.assert(&condition.implies(&var[0]._eq(&var[1])));
    }

    /// Any qubit in an SLM trap must stay in place between stages
    fn constraint_fixed_slm(&self, solver: &Optimize) {
        for q in &self.qubits {
            // Loop over stages
            for (x_step, y_step, aod) in izip!(q.x.windows(2), q.y.windows(2), &q.aod) {
                DPQAVars::require_unchanged(solver, &aod.not(), x_step);
                DPQAVars::require_unchanged(solver, &aod.not(), y_step);
            }
        }
    }

    /// Rows and columns of the AOD grid must move together
    fn constraint_aod_move_together(&self, solver: &Optimize) {
        for q in &self.qubits {
            // Loop over stages
            for (c_step, r_step, aod) in izip!(q.c.windows(2), q.r.windows(2), &q.aod) {
                DPQAVars::require_unchanged(solver, &aod, c_step);
                DPQAVars::require_unchanged(solver, &aod, r_step);
            }
        }

        // If any two qubits are in the same AOD row, and the AOD row moves,
        // then the two qubits must end up in the same row of the grid (i.e.
        // at the same value of y), and similarly for columns.
        let context = solver.get_context();
        let cr_eq_implies_xy_eq =
            |start_aod: &ast::Bool, cr: (&ast::Int, &ast::Int), xy: (&ast::Int, &ast::Int)| {
                let cr_eq = cr.0._eq(&cr.1);
                let cr_eq_aod = ast::Bool::and(context, &[start_aod, &cr_eq]);
                let xy_eq = xy.0._eq(&xy.1);
                solver.assert(&cr_eq_aod.implies(&xy_eq));
            };

        // Loop over pairs of distinct qubits
        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            let stages: Vec<(QubitVarsStage<'_>, QubitVarsStage<'_>)> =
                izip!(q0.iter(), q1.iter()).collect();
            for stage_pair in stages.windows(2) {
                let (curr, next) = (&stage_pair[0], &stage_pair[1]);
                let both_aod = ast::Bool::and(context, &[&curr.0.aod, &curr.1.aod]);
                cr_eq_implies_xy_eq(&both_aod, (curr.0.c, curr.1.c), (next.0.x, next.1.x));
                cr_eq_implies_xy_eq(&both_aod, (curr.0.r, curr.1.r), (next.0.y, next.1.y));
            }
        }
    }

    /// The order of AOD columns must be consistent with the order
    /// of SLM columns
    fn constraint_aod_order_from_slm(&self, solver: &Optimize) {
        let context = solver.get_context();
        let xy_lt_implies_cr_lt =
            |aod: &ast::Bool, xy: (&ast::Int, &ast::Int), cr: (&ast::Int, &ast::Int)| {
                let xy_lt = xy.0.lt(&xy.1);
                let enforce_lt = ast::Bool::and(context, &[aod, &xy_lt]);
                let cr_lt = cr.0.lt(&cr.1);
                solver.assert(&enforce_lt.implies(&cr_lt));
            };

        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            for vars in izip!(q0.iter(), q1.iter()) {
                let both_aod = ast::Bool::and(context, &[&vars.0.aod, &vars.1.aod]);

                xy_lt_implies_cr_lt(&both_aod, (vars.0.x, vars.1.x), (vars.0.c, vars.1.c));
                xy_lt_implies_cr_lt(&both_aod, (vars.1.x, vars.0.x), (vars.1.c, vars.0.c));

                xy_lt_implies_cr_lt(&both_aod, (vars.0.y, vars.1.y), (vars.0.r, vars.1.r));
                xy_lt_implies_cr_lt(&both_aod, (vars.1.y, vars.0.y), (vars.1.r, vars.0.r));
            }
        }
    }

    /// The order of SLM columns must be consistent with the order
    /// of AOD columns
    fn constraint_slm_order_from_aod(&self, solver: &Optimize) {
        let context = solver.get_context();
        let cr_lt_implies_xy_le =
            |aod: &ast::Bool, cr: (&ast::Int, &ast::Int), xy: (&ast::Int, &ast::Int)| {
                let cr_lt = cr.0.lt(&cr.1);
                let enforce_le = ast::Bool::and(context, &[&aod, &cr_lt]);
                let xy_le = xy.0.le(&xy.1);
                solver.assert(&enforce_le.implies(&xy_le));
            };

        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            let stages: Vec<(QubitVarsStage<'_>, QubitVarsStage<'_>)> =
                izip!(q0.iter(), q1.iter()).collect();
            for stage_pair in stages.windows(2) {
                let (curr, next) = (&stage_pair[0], &stage_pair[1]);
                let both_aod = ast::Bool::and(context, &[&curr.0.aod, &curr.1.aod]);

                cr_lt_implies_xy_le(&both_aod, (curr.0.c, curr.1.c), (next.0.x, next.1.x));
                cr_lt_implies_xy_le(&both_aod, (curr.1.c, curr.0.c), (next.1.x, next.0.x));

                cr_lt_implies_xy_le(&both_aod, (curr.0.r, curr.1.r), (next.0.y, next.1.y));
                cr_lt_implies_xy_le(&both_aod, (curr.1.r, curr.0.r), (next.1.y, next.0.y));
            }
        }
    }

    /// Prevent stacking/crowding of more than 3 AOD rows/columns
    fn constraint_aod_crowding(&self, solver: &Optimize) {
        let context = solver.get_context();
        let max_stack = ast::Int::from_u64(&context, 3);

        let cr_diff_implies_xy_gt =
            |aod: &ast::Bool, cr: (&ast::Int, &ast::Int), xy: (&ast::Int, &ast::Int)| {
                let cr_diff = ast::Int::sub(context, &[cr.0, cr.1]).ge(&max_stack);
                let enforce_gt = ast::Bool::and(context, &[&aod, &cr_diff]);
                let xy_gt = xy.0.gt(&xy.1);
                solver.assert(&enforce_gt.implies(&xy_gt));
            };

        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            // Constraint for initial stage
            {
                let both_aod = ast::Bool::and(context, &[&q0.aod[0], &q1.aod[0]]);
                cr_diff_implies_xy_gt(&both_aod, (&q0.c[0], &q1.c[0]), (&q0.x[0], &q1.x[0]));
                cr_diff_implies_xy_gt(&both_aod, (&q1.c[0], &q0.c[0]), (&q1.x[0], &q0.x[0]));
                cr_diff_implies_xy_gt(&both_aod, (&q0.r[0], &q1.r[0]), (&q0.y[0], &q1.y[0]));
                cr_diff_implies_xy_gt(&both_aod, (&q1.r[0], &q0.r[0]), (&q1.y[0], &q0.y[0]));
            }

            let stages: Vec<(QubitVarsStage<'_>, QubitVarsStage<'_>)> =
                izip!(q0.iter(), q1.iter()).collect();
            for stage_pair in stages.windows(2) {
                let (curr, next) = (&stage_pair[0], &stage_pair[1]);
                let both_aod = ast::Bool::and(context, &[&curr.0.aod, &curr.1.aod]);
                cr_diff_implies_xy_gt(&both_aod, (curr.0.c, curr.1.c), (next.0.x, next.1.x));
                cr_diff_implies_xy_gt(&both_aod, (curr.0.r, curr.1.r), (next.0.y, next.1.y));
                cr_diff_implies_xy_gt(&both_aod, (curr.1.c, curr.0.c), (next.1.x, next.0.x));
                cr_diff_implies_xy_gt(&both_aod, (curr.1.r, curr.0.r), (next.1.y, next.0.y));
            }
        }
    }

    /// Limit traps to one atom at a time
    fn constraint_site_crowding(&self, solver: &Optimize) {
        let context = solver.get_context();

        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            for (v0, v1) in izip!(q0.iter(), q1.iter()) {
                let both_aod = ast::Bool::and(context, &[v0.aod, v1.aod]);
                let cr_diff =
                    ast::Bool::or(context, &[&v0.c._eq(v1.c).not(), &v0.r._eq(v1.r).not()]);
                solver.assert(&both_aod.implies(&cr_diff));

                let both_slm = ast::Bool::and(context, &[&v0.aod.not(), &v1.aod.not()]);
                let xy_diff =
                    ast::Bool::or(context, &[&v0.x._eq(v1.x).not(), &v0.y._eq(v1.y).not()]);
                solver.assert(&both_slm.implies(&xy_diff));
            }
        }
    }

    /// Only allow AOD-SLM transfer when there is one atom at a given site
    fn constraint_no_swap(&self, solver: &Optimize) {
        let context = solver.get_context();

        for (q0, q1) in self.qubits.iter().tuple_combinations() {
            let stages: Vec<(QubitVarsStage<'_>, QubitVarsStage<'_>)> =
                izip!(q0.iter(), q1.iter()).collect();
            for stage_pair in stages.windows(2) {
                let (curr, next) = (&stage_pair[0], &stage_pair[1]);
                let same_site =
                    ast::Bool::and(context, &[&next.0.x._eq(next.1.x), &next.0.y._eq(next.1.y)]);
                let no_swap = ast::Bool::and(
                    context,
                    &[&curr.0.aod._eq(&next.0.aod), &curr.1.aod._eq(&next.1.aod)],
                );
                solver.assert(&same_site.implies(&no_swap));
            }
        }
    }

    /// Restrict each gate time to 0 <= t < self.n_stages, and ensure that
    /// gates with dependencies on each other are run in the right order
    pub fn constraint_t_bounds(&self, solver: &Optimize) {
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
    pub fn constraint_entangling_gates(&self, solver: &Optimize) {
        let context = solver.get_context();
        for (g, t) in izip!(self.circuit.iter(), self.t.iter()) {
            let (q0, q1) = (&self.qubits[g.q_ctrl], &self.qubits[g.q_target]);
            for (v0, v1, stage) in izip!(q0.iter(), q1.iter(), &self.s_vals) {
                let same_pos = ast::Bool::and(&context, &[&v0.x._eq(&v1.x), &v0.y._eq(&v1.y)]);
                solver.assert(&t._eq(&stage).implies(&same_pos));
            }
        }
    }

    /// Two qubits may only be at the same grid position if they are both
    /// used by a gate
    pub fn constraint_interaction_exactness(&self, solver: &Optimize) {
        // Maps a pair of qubits q0, q1 (with q0 < q1) to the indices of the
        // gate(s) that act on q0 and q1
        let mut interactions: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
        for (ii, g) in self.circuit.iter().enumerate() {
            let (q0, q1) = (g.q_ctrl.min(g.q_target), g.q_target.max(g.q_ctrl));
            interactions.entry((q0, q1)).or_default().push(ii);
        }

        let context = solver.get_context();

        for ((ii0, q0), (ii1, q1)) in self.qubits.iter().enumerate().tuple_combinations() {
            if let Some(gate_indices) = interactions.get(&(ii0, ii1)) {
                // This pair of qubits can interact, but only at stages
                // where both are used in a gate
                for (v0, v1, stage) in izip!(q0.iter(), q1.iter(), self.s_vals.iter()) {
                    let qubits_coincident =
                        ast::Bool::and(context, &[&v0.x._eq(&v1.x), &v0.y._eq(&v1.y)]);
                    let or_args: Vec<ast::Bool> = gate_indices
                        .iter()
                        .map(|&gg| self.t[gg]._eq(&stage))
                        .collect();
                    let gate_condition = ast::Bool::or(
                        &context,
                        or_args.iter().collect::<Vec<&ast::Bool>>().as_slice(),
                    );
                    solver.assert(&qubits_coincident.implies(&gate_condition));
                }
            } else {
                // This pair of qubits cannot interact
                for (v0, v1) in izip!(q0.iter(), q1.iter()) {
                    let qubits_not_coincident =
                        ast::Bool::or(&context, &[&v0.x._eq(&v1.x).not(), &v0.y._eq(&v1.y).not()]);
                    solver.assert(&qubits_not_coincident);
                }
            }
        }
    }

    /// If two gates are run at the same time, they must have the same type
    fn constraint_gate_type_timing(&self, solver: &Optimize) {
        for ((ii0, g0), (ii1, g1)) in self.circuit.iter().enumerate().tuple_combinations() {
            if g0.gate_type != g1.gate_type {
                solver.assert(&self.t[ii0]._eq(&self.t[ii1]).not());
            }
        }
    }

    /// Set all constraints
    pub fn set_constraints(&self, solver: &Optimize) {
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
        self.constraint_gate_type_timing(solver);
    }

    /// Minimize the number of moves between trap types
    fn minimize_transfers(&self, solver: &Optimize) {
        if self.t.len() == 1 {
            // If there is only one stage, there are no transfers
            return;
        }
        let context = solver.get_context();

        let transferred: Vec<ast::Int<'_>> = self
            .qubits
            .iter()
            .flat_map(|q| {
                q.aod.windows(2).map(|step| {
                    let (curr, next) = (&step[0], &step[1]);
                    curr._eq(&next).ite(&self.zero, &self.one)
                })
            })
            .collect();
        let refs: Vec<&ast::Int> = transferred.iter().map(|v| v).collect();

        let n_transfers = ast::Int::add(context, refs.as_slice());
        solver.minimize(&n_transfers);
    }

    /// Keep atoms in the stationary traps if possible
    fn prefer_slm(&self, solver: &Optimize) {
        let context = solver.get_context();

        let in_aod: Vec<ast::Int<'_>> = self
            .qubits
            .iter()
            .flat_map(|q| q.aod.iter().map(|trap| trap.ite(&self.one, &self.zero)))
            .collect();
        let refs: Vec<&ast::Int> = in_aod.iter().map(|v| v).collect();

        let aod_total = ast::Int::add(context, refs.as_slice());
        solver.minimize(&aod_total);
    }

    /// Set optimization targets
    pub fn set_optimization(&self, solver: &Optimize) {
        self.minimize_transfers(solver);
        self.prefer_slm(solver);
    }

    /// Get the qubit positions and gate execution times. Panics
    /// if solver state != Sat.
    pub fn eval(&self, solver: &Optimize) -> DPQAVarsValues {
        let model = solver.get_model().unwrap();

        let get_u64 = |var: &ast::Int| -> u64 { model.eval(var, true).unwrap().as_u64().unwrap() };

        let xy_result = self
            .qubits
            .iter()
            .map(|q| {
                izip!(&q.x, &q.y)
                    .map(|(x, y)| (get_u64(x), get_u64(y)))
                    .collect()
            })
            .collect();

        let cr_result = self
            .qubits
            .iter()
            .map(|q| {
                izip!(&q.c, &q.r)
                    .map(|(c, r)| (get_u64(c), get_u64(r)))
                    .collect()
            })
            .collect();

        let aod_result = self
            .qubits
            .iter()
            .map(|q| {
                q.aod
                    .iter()
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
