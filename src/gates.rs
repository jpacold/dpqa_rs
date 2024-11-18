use std::fmt;

// Commutation relations for basic two-qubit gates

#[derive(PartialEq, Eq, Clone, Copy, Debug)]
pub enum TwoQubitGateType {
    CX,
    CZ,
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub struct TwoQubitGate {
    pub gate_type: TwoQubitGateType,
    pub q_ctrl: usize,
    pub q_target: usize,
}

impl TwoQubitGate {
    pub fn new(gate_type: TwoQubitGateType, q_ctrl: usize, q_target: usize) -> TwoQubitGate {
        TwoQubitGate {
            gate_type,
            q_ctrl,
            q_target,
        }
    }

    pub fn parallel_with(&self, gate: &TwoQubitGate) -> bool {
        self.q_ctrl != gate.q_ctrl
            && self.q_ctrl != gate.q_target
            && self.q_target != gate.q_ctrl
            && self.q_target != gate.q_target
    }

    pub fn commutes_with(&self, gate: &TwoQubitGate) -> bool {
        if self.parallel_with(gate) {
            return true;
        }

        match self.gate_type {
            TwoQubitGateType::CX => match gate.gate_type {
                TwoQubitGateType::CX => {
                    self.q_ctrl != gate.q_target && self.q_target != gate.q_ctrl
                }
                TwoQubitGateType::CZ => self.q_target != gate.q_target,
            },

            TwoQubitGateType::CZ => match gate.gate_type {
                TwoQubitGateType::CX => self.q_target != gate.q_target,
                TwoQubitGateType::CZ => true,
            },
        }
    }
}

impl fmt::Display for TwoQubitGate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let gate_name = match self.gate_type {
            TwoQubitGateType::CX => "CX",
            TwoQubitGateType::CZ => "CZ",
        };

        write!(f, "{}({}, {})", gate_name, self.q_ctrl, self.q_target)
    }
}

impl fmt::Debug for TwoQubitGate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self)
    }
}
