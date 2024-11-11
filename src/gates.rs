use std::fmt;

// Commutation relations for basic two-qubit gates

pub enum TwoQubitGateType {
    CX,
    CZ,
}

pub struct TwoQubitGate {
    pub gate_type: TwoQubitGateType,
    pub q_ctrl: u32,
    pub q_target: u32,
}

impl TwoQubitGate {
    pub fn new(gate_type: TwoQubitGateType, q_ctrl: u32, q_target: u32) -> TwoQubitGate {
        TwoQubitGate {
            gate_type,
            q_ctrl,
            q_target,
        }
    }

    pub fn commutes_with(&self, gate: &TwoQubitGate) -> bool {
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

    pub fn parallel_with(&self, gate: &TwoQubitGate) -> bool {
        self.q_ctrl != gate.q_ctrl
            && self.q_ctrl != gate.q_target
            && self.q_target != gate.q_ctrl
            && self.q_target != gate.q_target
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
