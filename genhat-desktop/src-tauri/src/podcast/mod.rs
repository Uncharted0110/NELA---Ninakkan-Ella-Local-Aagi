//! Podcast Mode — RAG → Script Generation → Multi-Voice TTS → Audio Merge.
//!
//! Pipeline:
//!   1. RAG retrieval: query the knowledge base for relevant context
//!   2. Script generation: LLM produces a two-person dialogue as JSON
//!   3. TTS synthesis: each dialogue line is synthesized with a different voice
//!   4. Audio merge: WAV segments are concatenated into a single podcast file
//!
//! All stages emit progress events to the frontend via Tauri events.

pub mod types;
pub mod script;
pub mod engine;
