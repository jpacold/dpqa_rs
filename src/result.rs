use crate::instruction::DPQAInstruction;

/// Compilation result object
#[derive(PartialEq, Eq, Debug)]
pub enum DPQAResult {
    Failed,
    Succeeded(Vec<DPQAInstruction>),
}
