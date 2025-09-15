//! Collection of feedbacks for diffray: return-value and coverage variants
pub mod coverage;
pub mod noop;
pub mod return_value;
pub use return_value::ReturnValueFeedback;

pub mod objectives;
pub mod two_state_map;
pub use objectives::DiffRayObjective;
