pub mod error;
pub mod model;

pub use error::TimelineError;
pub use model::{
    CanvasConfig, ClipTransform, TimelineClip, TimelineDocument, TimelineTrack, TrackKind,
};
