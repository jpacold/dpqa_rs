# Rust DPQA compiler

This is a Rust version of a compiler for a 2D dynamically field-programmable qubit array (DPQA), described by [Tan et al. (2024)](https://arxiv.org/abs/2306.03487). The Python version of the compiler referenced in the paper can be found at [UCLA-VAST/DPQA](https://github.com/UCLA-VAST/DPQA).

## Example

Consider a 2D grid which can have up to two qubits at each site. Qubits that occupy the same site are close enough to execute entangling gates. Given a way to physically move qubits around the grid, we can reduce the number of SWAP gates needed to run a given set of operations.

For example, suppose we have an 8-qubit circuit and want to run two-qubit gates on the following pairs:
```
(0, 2), (1, 3),
(0, 4), (1, 5),
(0, 6), (1, 7)
```
One way to do this is to keep qubits 2 through 7 fixed, while moving qubits 0 and 1. We can run the solver for this problem as follows:

```rust
use super::{DPQAResult, DPQA};
use crate::circuit::Circuit;
use crate::gates::TwoQubitGate;
// CZ is currently a placeholder for a generic two-qubit gate
use crate::gates::TwoQubitGateType::CZ;

// Define the sequence of gates
let mut circuit = Circuit::new();
circuit.append(TwoQubitGate::new(CZ, 0, 2));
circuit.append(TwoQubitGate::new(CZ, 1, 3));
circuit.append(TwoQubitGate::new(CZ, 0, 4));
circuit.append(TwoQubitGate::new(CZ, 1, 5));
circuit.append(TwoQubitGate::new(CZ, 0, 6));
circuit.append(TwoQubitGate::new(CZ, 1, 7));
circuit.recalculate_stages();

// Run the solver for a grid with 3 rows and two columns
let dpqa = DPQA::new(3, 2);
let result = dpqa.solve(&circuit);
```
The result object contains a vector of `DPQAInstruction` objects that describe how to initialize and run the circuit:
```
Initialize qubit 0 at x=0, y=2 (AOD)
Initialize qubit 1 at x=1, y=2 (AOD)
Initialize qubit 2 at x=0, y=2 (SLM)
Initialize qubit 3 at x=1, y=2 (SLM)
Initialize qubit 4 at x=0, y=0 (SLM)
Initialize qubit 5 at x=1, y=0 (AOD)
Initialize qubit 6 at x=0, y=1 (SLM)
Initialize qubit 7 at x=1, y=1 (SLM)
Execute gate on qubit pair(s) [(0, 2), (1, 3)]
Move qubit row [0, 1] from y=2 to y=0
Execute gate on qubit pair(s) [(0, 4), (1, 5)]
Move qubit row [0, 1] from y=0 to y=1
Execute gate on qubit pair(s) [(0, 6), (1, 7)]
```

## Note
Tan et al. describe two compilation strategies for this architecture: an optimal approach for small circuits, and a hybrid greedy/optimal algorithm for large circuits. So far only the optimal approach is implemented here.
