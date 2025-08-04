mod accelerator;
mod codec;
mod error;
// mod filter;
mod input;
mod player;
mod stream;

use rusty_ffmpeg::ffi as ffmpeg;

pub use self::accelerator::{Accelerator, AcceleratorConfig};
pub use self::error::{FFmpegError, PlayerError, Result};
pub use self::input::InputSource;
pub use self::player::{DecodedFrame, MediaPlayer, MediaPlayerBuilder};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum MediaType {
    /// Video data.
    Video,
    /// Audio data.
    Audio,
    /// Subtitle data.
    Subtitle,
    /// Opaque data information usually continuous.
    Data,
    /// Opaque data information usually sparse.
    Attachment,
    /// Usually treated as AVMEDIA_TYPE_DATA.
    Unknown,
}

impl MediaType {
    pub(crate) fn to_av_media_type(&self) -> ffmpeg::AVMediaType {
        match self {
            MediaType::Video => ffmpeg::AVMEDIA_TYPE_VIDEO,
            MediaType::Audio => ffmpeg::AVMEDIA_TYPE_AUDIO,
            MediaType::Subtitle => ffmpeg::AVMEDIA_TYPE_SUBTITLE,
            MediaType::Data => ffmpeg::AVMEDIA_TYPE_DATA,
            MediaType::Attachment => ffmpeg::AVMEDIA_TYPE_ATTACHMENT,
            MediaType::Unknown => ffmpeg::AVMEDIA_TYPE_UNKNOWN,
        }
    }
}

impl From<ffmpeg::AVMediaType> for MediaType {
    fn from(value: ffmpeg::AVMediaType) -> Self {
        match value {
            ffmpeg::AVMEDIA_TYPE_VIDEO => Self::Video,
            ffmpeg::AVMEDIA_TYPE_AUDIO => Self::Audio,
            ffmpeg::AVMEDIA_TYPE_SUBTITLE => Self::Subtitle,
            ffmpeg::AVMEDIA_TYPE_DATA => Self::Data,
            ffmpeg::AVMEDIA_TYPE_ATTACHMENT => Self::Attachment,
            _ => Self::Unknown,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
/// The pixel format describes how image data is organized and represented.
///
/// This is used as a _target_ for rendering output, it is normally recommended
/// to use [OutputPixelFormat::Nv12] as other formats may lack hardware decoding
/// support or require significantly more bandwidth to copy across.
pub enum OutputPixelFormat {
    #[default]
    /// The most common output format for video output and rendering.
    ///
    /// This is by far the least bandwidth & compute intensive process
    /// and should be the default for most situations.
    Nv12,
    /// A common output format for rendering UIs.
    ///
    /// This is mostly for compatibility, but this is not suitable for
    /// high frame rate videos as it is significantly (2x or more) bandwidth
    /// intensive compared to NV12.
    Rgba,
    /// The common output format for HDR10 / Dolby Vision.
    ///
    /// This output format is _very_ heavily and requires a significant amount of
    /// GPU and CPU power so it is not suitable for most applications if you don't
    /// want HDR output specifically (note your renderer needs to support HDR itself!)
    ///
    /// This format is more bandwidth intensive than RGBA and NV12, high FPS videos can
    /// have significant compute costs.
    P010le,
}

impl OutputPixelFormat {
    pub(crate) fn to_filter_name(&self) -> &'static str {
        match self {
            OutputPixelFormat::Nv12 => "nv12",
            OutputPixelFormat::Rgba => "rgba",
            OutputPixelFormat::P010le => "p010le",
        }
    }
}
