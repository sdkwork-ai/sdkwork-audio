//! Database entities for audio generation.
//!
//! Task/artifact/voice entities are shared service-port types owned by
//! `sdkwork-audio-generation-service-port` and re-exported here so existing
//! consumers keep working. Event and provider entities remain local to this
//! SQLx repository crate.

pub use sdkwork_audio_generation_service_port::entities::{artifact, task, voice};
pub use sdkwork_audio_generation_service_port::entities::{artifact::*, task::*, voice::*};

pub mod event;
pub mod provider;

pub use event::*;
pub use provider::*;