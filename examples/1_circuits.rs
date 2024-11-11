use dpqa_rs::circuit::Circuit;
use dpqa_rs::gates::TwoQubitGate;
use dpqa_rs::gates::TwoQubitGateType::{CX, CZ};

pub fn main() {
    let mut circuit = Circuit::new();
    circuit.add(TwoQubitGate::new(CZ, 0, 1));
    circuit.add(TwoQubitGate::new(CX, 1, 3));

    println!("{}", circuit);
}
