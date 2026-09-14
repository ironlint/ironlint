//! Engine module: the single gate-execution model.

mod execution;
pub mod gate;

pub(crate) use execution::{
    run_v1, V1ExecutionEnv, V1ExecutionError, V1ExecutionOutcome, V1ExecutionResult,
};
pub use gate::{run_gate, GateEnv, GateOutcome, InternalReason};
