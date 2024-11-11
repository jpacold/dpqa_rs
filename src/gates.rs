use std::fmt;

// Commutation relations for basic two-qubit gates

pub enum TwoQubitGateType {
    CX,
    CZ,
}

pub struct TwoQubitGate {
    gate_type: TwoQubitGateType,
    qubits: (u32, u32),
}

impl TwoQubitGate {
    pub fn new(gate_type: TwoQubitGateType, qubits: (u32, u32)) -> TwoQubitGate {
        TwoQubitGate { gate_type, qubits }
    }

    pub fn commutes_with(&self, gate: &TwoQubitGate) -> bool {
        match self.gate_type {
            TwoQubitGateType::CX => match gate.gate_type {
                TwoQubitGateType::CX => {
                    self.qubits.0 != gate.qubits.1 && self.qubits.1 != gate.qubits.0
                }
                TwoQubitGateType::CZ => self.qubits.1 != gate.qubits.1,
            },

            TwoQubitGateType::CZ => match gate.gate_type {
                TwoQubitGateType::CX => self.qubits.1 != gate.qubits.1,
                TwoQubitGateType::CZ => true,
            },
        }
    }

    pub fn parallel_with(&self, gate: &TwoQubitGate) -> bool {
        self.qubits.0 != gate.qubits.0
            && self.qubits.0 != gate.qubits.1
            && self.qubits.1 != gate.qubits.0
            && self.qubits.1 != gate.qubits.1
    }
}

impl fmt::Display for TwoQubitGate {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let gate_name = match self.gate_type {
            TwoQubitGateType::CX => "CX",
            TwoQubitGateType::CZ => "CZ",
        };

        write!(f, "{}({}, {})", gate_name, self.qubits.0, self.qubits.1)
    }
}
