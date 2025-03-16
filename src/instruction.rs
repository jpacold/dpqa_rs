use crate::gates::TwoQubitGate;
use std::fmt;

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
    MoveToAOD(usize),
    Gate(Vec<TwoQubitGate>),
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
            DPQAInstruction::MoveToSLM(qubit) => write!(f, "Transfer qubit {} to SLM", qubit),
            DPQAInstruction::MoveToAOD(qubit) => write!(f, "Transfer qubit {} to AOD", qubit),
            DPQAInstruction::Gate(qubit_pairs) => {
                write!(f, "Execute {:?}", qubit_pairs)
            }
        }
    }
}
