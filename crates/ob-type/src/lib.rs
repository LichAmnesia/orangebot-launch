//! orangebot type: voice typing tool (dictate, translate, ask).

pub mod ask;
pub mod prompts;
pub mod rewrite;
pub mod state;
pub mod style;
pub mod tool;

pub use ask::*;
pub use prompts::*;
pub use rewrite::*;
pub use state::*;
pub use style::*;
pub use tool::*;
