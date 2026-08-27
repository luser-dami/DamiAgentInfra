//! The knowledge engine: document chunking, compile pipeline, contract gate,
//! lint, retrieval, and feedback — one library per project, packs alongside.

mod chunk;
pub(crate) mod compile;
mod doctor;
mod contract;
mod embed;
pub(crate) mod eval;
mod extract;
mod feedback;
mod lint;
mod packet;
mod retrieve;
pub(crate) mod schema;
mod tidy;

pub use compile::{compile_index, compile_pack};
pub use doctor::run as doctor;
pub use contract::{compile_health_report, contract_report, contract_value};
pub use embed::make_embedder;
pub use feedback::{clear as feedback_clear, list as feedback_list, record as feedback_record};
pub use lint::lint;
pub use retrieve::{locate, query, refs, status};
pub use tidy::{emit as tidy_emit, tidy_docs};
pub(super) use compile::{claim_grade_counts, count, count_status};
