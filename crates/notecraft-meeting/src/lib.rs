//! Meeting notes: records the microphone and turns speech into text on this Mac with Whisper.
//! Nothing here talks to the network except the one-time download of a speech model.

pub mod audio;
pub mod chunker;
pub mod diarize;
pub mod models;
pub mod permission;
pub mod transcribe;

pub use audio::{input_devices, Recorder, Source};
pub use models::{WhisperModel, MODELS};
pub use transcribe::{Event, Transcriber};

/// Sample rate Whisper expects.
pub const SAMPLE_RATE: usize = 16_000;
