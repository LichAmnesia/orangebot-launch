//! OpenAI-compatible clients: chat, audio transcription, model listing.

pub mod client;
pub mod search;

pub use client::*;
pub use search::*;
