use std::fmt::Formatter;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::MediaType;

/// A single immutable audio, video or subtitle stream from an [InputSource](crate::InputSource).
pub struct StreamInfo {
    /// The media type of the stream.
    pub media_type: MediaType,
    /// Returns the index position of the stream.
    pub index: usize,
    /// Returns the frame rate of the stream.
    pub framerate: FrameRate,
    /// Returns the resolution of the stream, providing it is a
    /// video stream.
    pub resolution: Option<Resolution>,
    /// Returns the bitrate of the stream in kilobits per second if available.
    ///
    /// Some containers like MKV might not make this available without probing the stream
    /// and estimating the bitrate which this method does not provide.
    pub bitrate: Option<usize>,
    /// Returns the name of the media codec this stream uses.
    pub codec_name: String,
}

impl StreamInfo {
    /// Creates a new [StreamInfo] using the given raw pointer.
    pub(crate) unsafe fn from_raw(ctx: *const ffmpeg::AVStream) -> Self {
        assert!(!ctx.is_null());

        let stream = unsafe { &*ctx };
        let codec_params = unsafe { &*stream.codecpar };

        let media_type = MediaType::from(codec_params.codec_type);
        let index = stream.index as usize;
        let framerate = FrameRate::new(
            stream.avg_frame_rate.num as usize,
            stream.avg_frame_rate.den as usize,
        );

        let mut resolution = None;
        if media_type == MediaType::Video {
            resolution = Some(Resolution {
                width: codec_params.width as usize,
                height: codec_params.height as usize,
            });
        }

        let mut bitrate = None;
        if codec_params.bit_rate > 0 {
            bitrate = Some(codec_params.bit_rate as usize);
        }

        let stream_codec =
            unsafe { ffmpeg::avcodec_find_decoder((*codec_params).codec_id) };
        let codec_name = if stream_codec.is_null() {
            "unknown".to_string()
        } else {
            let raw_name = unsafe { std::ffi::CStr::from_ptr((*stream_codec).name) };
            let str_view = raw_name.to_string_lossy();
            str_view.to_string()
        };

        Self {
            media_type,
            index,
            framerate,
            resolution,
            bitrate,
            codec_name,
        }
    }
}

#[derive(Copy, Clone)]
/// The frame rate of a given stream.
///
/// This is represented in the form of a numerator and a denominator.
pub struct FrameRate {
    numerator: usize,
    denominator: usize,
}

impl std::fmt::Debug for FrameRate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "FrameRate({}/{}, fps={:.2})",
            self.numerator(),
            self.denominator(),
            self.as_f32()
        )
    }
}

impl FrameRate {
    /// Creates a new [FrameRate] using the given fractional components.
    pub(crate) fn new(numerator: usize, denominator: usize) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    #[inline]
    /// Returns the rate in terms of frames per second as a f32 value.
    pub fn as_f32(&self) -> f32 {
        self.numerator as f32 / self.denominator as f32
    }

    #[inline]
    /// Returns the numerator part of the fraction.
    pub fn numerator(&self) -> usize {
        self.numerator
    }

    #[inline]
    /// Returns the denominator part of the fraction.
    pub fn denominator(&self) -> usize {
        self.denominator
    }
}

impl Eq for FrameRate {}

impl PartialEq for FrameRate {
    fn eq(&self, other: &Self) -> bool {
        self.numerator() == other.numerator()
            && self.denominator() == other.denominator()
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq, Default)]
/// The resolution of a video stream.
pub struct Resolution {
    /// The width of the video resolution in pixels.
    pub width: usize,
    /// The height of the video resolution in pixels.
    pub height: usize,
}
