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
    pub(crate) fn try_from_av_pix_fmt(fmt: ffmpeg::AVPixelFormat) -> Option<Self> {
        match fmt {
            ffmpeg::AV_PIX_FMT_NV12 => Some(Self::Nv12),
            ffmpeg::AV_PIX_FMT_RGBA => Some(Self::Rgba),
            ffmpeg::AV_PIX_FMT_P010LE => Some(Self::P010le),
            _ => None,
        }
    }

    pub(crate) fn descriptor(&self) -> Option<&'static ffmpeg::AVPixFmtDescriptor> {
        let av_pix_fmt = match self {
            OutputPixelFormat::Nv12 => ffmpeg::AV_PIX_FMT_NV12,
            OutputPixelFormat::Rgba => ffmpeg::AV_PIX_FMT_RGBA,
            OutputPixelFormat::P010le => ffmpeg::AV_PIX_FMT_P010LE,
        };
        unsafe {
            let descriptor = ffmpeg::av_pix_fmt_desc_get(av_pix_fmt);
            descriptor.as_ref()
        }
    }

    pub(crate) fn to_filter_name(&self) -> &'static str {
        match self {
            OutputPixelFormat::Nv12 => "nv12",
            OutputPixelFormat::Rgba => "rgba",
            OutputPixelFormat::P010le => "p010le",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
/// The audio sample format.
pub enum SampleFormat {
    /// Unsigned 8 bit.
    U8,
    /// Signed 16 bit.
    S16,
    /// Signed 32 bit.
    S32,
    /// Single precision float.
    FLT,
    /// Double precision float.
    DBL,
    /// Unsigned 8 bit, planar.
    U8P,
    /// Signed 16 bit, planar.
    S16P,
    /// Signed 32 bit, planar.
    S32P,
    /// Single precision float, planar.
    FLTP,
    /// Double precision float, planar.
    DBLP,
    /// Signed 64 bit.
    S64,
    /// Signed 64 bit, planar.
    S64P,
}

impl SampleFormat {
    #[inline]
    /// Returns if the sample is planar.
    pub fn is_planar(&self) -> bool {
        unsafe { ffmpeg::av_sample_fmt_is_planar(self.to_av_sample_fmt()) == 1 }
    }

    #[inline]
    /// Returns if the sample is packed.
    pub fn is_packed(&self) -> bool {
        !self.is_planar()
    }

    #[inline]
    pub(crate) fn try_from_av_sample_fmt(fmt: ffmpeg::AVSampleFormat) -> Option<Self> {
        match fmt {
            ffmpeg::AV_SAMPLE_FMT_U8 => Some(Self::U8),
            ffmpeg::AV_SAMPLE_FMT_S16 => Some(Self::S16),
            ffmpeg::AV_SAMPLE_FMT_S32 => Some(Self::S32),
            ffmpeg::AV_SAMPLE_FMT_FLT => Some(Self::FLT),
            ffmpeg::AV_SAMPLE_FMT_DBL => Some(Self::DBL),
            ffmpeg::AV_SAMPLE_FMT_U8P => Some(Self::U8P),
            ffmpeg::AV_SAMPLE_FMT_S16P => Some(Self::S16P),
            ffmpeg::AV_SAMPLE_FMT_S32P => Some(Self::S32P),
            ffmpeg::AV_SAMPLE_FMT_FLTP => Some(Self::FLTP),
            ffmpeg::AV_SAMPLE_FMT_DBLP => Some(Self::DBLP),
            ffmpeg::AV_SAMPLE_FMT_S64 => Some(Self::S64),
            ffmpeg::AV_SAMPLE_FMT_S64P => Some(Self::S64P),
            _ => None,
        }
    }

    #[inline]
    pub(crate) fn to_av_sample_fmt(&self) -> ffmpeg::AVSampleFormat {
        match self {
            Self::U8 => ffmpeg::AV_SAMPLE_FMT_U8,
            Self::S16 => ffmpeg::AV_SAMPLE_FMT_S16,
            Self::S32 => ffmpeg::AV_SAMPLE_FMT_S32,
            Self::FLT => ffmpeg::AV_SAMPLE_FMT_FLT,
            Self::DBL => ffmpeg::AV_SAMPLE_FMT_DBL,
            Self::U8P => ffmpeg::AV_SAMPLE_FMT_U8P,
            Self::S16P => ffmpeg::AV_SAMPLE_FMT_S16P,
            Self::S32P => ffmpeg::AV_SAMPLE_FMT_S32P,
            Self::FLTP => ffmpeg::AV_SAMPLE_FMT_FLTP,
            Self::DBLP => ffmpeg::AV_SAMPLE_FMT_DBLP,
            Self::S64 => ffmpeg::AV_SAMPLE_FMT_S64,
            Self::S64P => ffmpeg::AV_SAMPLE_FMT_S64P,
        }
    }
}
