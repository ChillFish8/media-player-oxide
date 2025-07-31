

use rusty_ffmpeg::ffi as ffmpeg;

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