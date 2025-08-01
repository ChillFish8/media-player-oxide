use std::borrow::Cow;
use std::fmt::Formatter;
use std::marker::PhantomData;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::accelerator::AcceleratorConfig;
use crate::codec::VideoDecoder;
use crate::{MediaType, codec, error};

/// A single immutable audio, video or subtitle stream from an [InputSource](crate::InputSource).
pub struct Stream<'src> {
    ctx: *const ffmpeg::AVStream,
    phantom: PhantomData<&'src ()>,
}

impl<'src> Stream<'src> {
    /// Creates a new [Stream] using the given raw pointer.
    ///
    /// The provided pointer must not be null and must live as long as `'src`.
    pub(crate) unsafe fn from_raw(ctx: *const ffmpeg::AVStream) -> Self {
        assert!(!ctx.is_null());
        Self {
            ctx,
            phantom: PhantomData,
        }
    }

    #[inline]
    /// The media type of the stream.
    pub fn media_type(&self) -> MediaType {
        unsafe {
            let codec_params = (*self.ctx).codecpar;
            let media_type_raw = (*codec_params).codec_type;
            MediaType::from(media_type_raw)
        }
    }

    #[inline]
    /// Returns the index position of the stream.
    pub fn index(&self) -> usize {
        unsafe { (*self.ctx).index as usize }
    }

    #[inline]
    /// Returns the frame rate of the stream.
    pub fn framerate(&self) -> FrameRate {
        unsafe {
            let rate = (*self.ctx).avg_frame_rate;
            FrameRate::new(rate.num as usize, rate.den as usize)
        }
    }

    #[inline]
    /// Returns the resolution of the stream, providing it is a
    /// video stream.
    pub fn resolution(&self) -> Option<Resolution> {
        if self.media_type() != MediaType::Video {
            return None;
        }

        let mut resolution = Resolution::default();
        unsafe {
            let codec_params = (*self.ctx).codecpar;
            resolution.width = (*codec_params).width as usize;
            resolution.height = (*codec_params).height as usize;
        }

        Some(resolution)
    }

    /// Returns the bitrate of the stream in kilobits per second if available.
    ///
    /// Some containers like MKV might not make this available without probing the stream
    /// and estimating the bitrate which this method does not provide.
    pub fn bitrate(&self) -> Option<usize> {
        unsafe {
            let codec_params = (*self.ctx).codecpar;
            let bit_rate = (*codec_params).bit_rate;
            if bit_rate <= 0 {
                None
            } else {
                Some(bit_rate as usize / 1_000)
            }
        }
    }

    /// Returns the name of the media codec this stream uses.
    pub fn codec_name(&self) -> Cow<str> {
        unsafe {
            let codec_params = (*self.ctx).codecpar;
            let stream_codec = ffmpeg::avcodec_find_decoder((*codec_params).codec_id);
            if stream_codec.is_null() {
                return Cow::Borrowed("unknown");
            }

            let name = std::ffi::CStr::from_ptr((*stream_codec).name);
            name.to_string_lossy()
        }
    }

    #[inline]
    /// Returns the raw pointer this stream contains.
    ///
    /// WARNING: The lifetime os this pointer is tied directly to `'src`.
    pub(crate) fn as_ptr(&self) -> *const ffmpeg::AVStream {
        self.ctx
    }

    /// Attempt to get a video decoder for the stream using the given
    /// accelerator config and target pixel format.
    pub(crate) fn open_decoder(
        &self,
        target_pixel_format: crate::OutputPixelFormat,
        accelerator_config: &AcceleratorConfig,
    ) -> Result<VideoDecoder, error::FFmpegError> {
        let codec_params = unsafe { (*self.ctx).codecpar };
        let stream_codec =
            unsafe { ffmpeg::avcodec_find_decoder((*codec_params).codec_id) };

        if stream_codec.is_null() {
            return Err(error::FFmpegError::from_raw_errno(
                -ffmpeg::AVERROR_DECODER_NOT_FOUND,
            ));
        }

        codec::open_best_fitting_decoder(
            stream_codec,
            codec_params,
            target_pixel_format,
            accelerator_config,
        )
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
