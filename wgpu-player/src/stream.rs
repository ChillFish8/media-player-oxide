use std::fmt::Formatter;
use std::marker::PhantomData;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::media::MediaType;

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
        unsafe { FrameRate((*self.ctx).avg_frame_rate) }
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
                Some(bit_rate as usize)
            }
        }
    }

    #[inline]
    /// Returns the raw pointer this stream contains.
    ///
    /// WARNING: The lifetime os this pointer is tied directly to `'src`.
    pub(crate) fn as_ptr(&self) -> *const ffmpeg::AVStream {
        self.ctx
    }
}


#[derive(Copy, Clone)]
/// The frame rate of a given stream.
///
/// This is represented in the form of a numerator and a denominator.
pub struct FrameRate(ffmpeg::AVRational);

impl std::fmt::Debug for FrameRate {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "FrameRate({}/{}, fps={:.2})", self.numerator(), self.denominator(), self.as_f32())
    }
}

impl FrameRate {
    #[inline]
    /// Returns the rate in terms of frames per second as a f32 value.
    pub fn as_f32(&self) -> f32 {
        self.0.num as f32 / self.0.den as f32
    }

    #[inline]
    /// Returns the numerator part of the fraction.
    pub fn numerator(&self) -> usize {
        self.0.num as usize
    }

    #[inline]
    /// Returns the denominator part of the fraction.
    pub fn denominator(&self) -> usize {
        self.0.den as usize
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