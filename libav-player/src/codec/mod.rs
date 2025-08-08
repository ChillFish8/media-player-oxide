mod audio;
mod subtitle;
mod video;

use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

pub(crate) use self::audio::AudioDecoder;
pub(crate) use self::subtitle::SubtitleDecoder;
pub(crate) use self::video::VideoDecoder;
use crate::error;
use crate::stream::StreamInfo;

/// Find a ffmpeg codec by name.
///
/// Returns `None` if the codec does not exist.
pub(crate) fn find_decoder_by_name(name: &str) -> Option<&'static ffmpeg::AVCodec> {
    let name = std::ffi::CString::new(name).unwrap();
    let codec = unsafe { ffmpeg::avcodec_find_decoder_by_name(name.as_ptr()) };
    if codec.is_null() {
        None
    } else {
        Some(unsafe { &*codec })
    }
}

/// Find a ffmpeg codec by ID.
///
/// Returns `None` if the codec does not exist.
pub(crate) fn find_decoder_by_id(
    id: ffmpeg::AVCodecID,
) -> Option<&'static ffmpeg::AVCodec> {
    let codec = unsafe { ffmpeg::avcodec_find_decoder(id) };
    if codec.is_null() {
        None
    } else {
        Some(unsafe { &*codec })
    }
}

/// The decoder processes incoming packets and produces media frames.
pub(crate) trait Decoder: Sized {
    /// The frame type produced by the decoder;
    type Frame;

    /// Create a new decoder using the target codec.
    fn create(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
    ) -> Result<Self, error::FFmpegError>;

    /// Return a mutable reference to the internal decoder context.
    fn as_mut_ctx(&mut self) -> &mut ffmpeg::AVCodecContext;

    /// Return a reference to the internal decoder context.
    fn as_ctx(&self) -> &ffmpeg::AVCodecContext;

    /// Copy the provided codec parameters into the decoder context.
    ///
    /// This must be done before opening.
    fn copy_codec_params(
        &mut self,
        params: &ffmpeg::AVCodecParameters,
    ) -> Result<(), error::FFmpegError> {
        let result =
            unsafe { ffmpeg::avcodec_parameters_to_context(self.as_mut_ctx(), params) };
        error::convert_ff_result(result)?;
        Ok(())
    }

    fn is_open(&self) -> bool {
        let ctx = self.as_ctx() as *const ffmpeg::AVCodecContext;
        unsafe { ffmpeg::avcodec_is_open(ctx as *mut ffmpeg::AVCodecContext) == 1 }
    }

    /// Open and initialise the decoder.
    fn open(&mut self) -> Result<(), error::FFmpegError>;

    /// Signal to the decoder that all packets have been read,
    /// and it should flush any remaining data.
    fn flush(&mut self) -> Result<(), error::FFmpegError> {
        Ok(())
    }

    /// Push packet data into the decoder.
    fn write_packet(
        &mut self,
        packet: &mut ffmpeg::AVPacket,
    ) -> Result<(), error::FFmpegError> {
        let result = unsafe { ffmpeg::avcodec_send_packet(self.as_mut_ctx(), packet) };
        error::convert_ff_result(result)?;
        Ok(())
    }

    /// Attempt to decode a new frame and write it to the provided [Self::Frame].
    fn decode(&mut self, frame: &mut Self::Frame) -> Result<(), error::FFmpegError>;

    fn apply_context_to_frame(&self, frame: &mut Self::Frame);
}

/// A wrapper around a [ffmpeg::AVCodec] and context.
///
/// This implements the basic necessary logic for processing
/// media frames of any type and managing the lifecycle of the codec.
///
/// This codec does no hardware acceleration.
pub(crate) struct BaseDecoder {
    ctx: *mut ffmpeg::AVCodecContext,
    codec: &'static ffmpeg::AVCodec,
    is_open: bool,
}

impl BaseDecoder {
    /// Open a new [BaseDecoder] using the target codec and codec parameters.
    pub(crate) fn open(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
        codec_params: Option<&ffmpeg::AVCodecParameters>,
    ) -> Result<Self, error::FFmpegError> {
        let mut decoder = Self::create(codec, stream_info)?;
        if let Some(codec_params) = codec_params {
            decoder.copy_codec_params(codec_params)?;
        }
        decoder.open()?;
        Ok(decoder)
    }
}

impl Decoder for BaseDecoder {
    type Frame = ffmpeg::AVFrame;

    fn create(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
    ) -> Result<Self, error::FFmpegError> {
        let context = unsafe { ffmpeg::avcodec_alloc_context3(codec) };
        if context.is_null() {
            return Err(error::FFmpegError::custom(
                "failed to allocate codec context",
            ));
        }

        tracing::debug!("created decoder");

        let mut decoder = Self {
            ctx: context,
            codec,
            is_open: false,
        };

        let ctx = decoder.as_mut_ctx();
        ctx.time_base = stream_info.time_base.to_av_rational();

        Ok(decoder)
    }

    fn as_mut_ctx(&mut self) -> &mut ffmpeg::AVCodecContext {
        unsafe { &mut *self.ctx }
    }

    fn as_ctx(&self) -> &ffmpeg::AVCodecContext {
        unsafe { &*self.ctx }
    }

    fn open(&mut self) -> Result<(), error::FFmpegError> {
        // Open should never normally be called twice.
        if self.is_open {
            panic!("codec is already open");
        }

        let result =
            unsafe { ffmpeg::avcodec_open2(self.ctx, self.codec, ptr::null_mut()) };
        error::convert_ff_result(result)?;

        self.is_open = true;

        Ok(())
    }

    fn decode(&mut self, frame: &mut Self::Frame) -> Result<(), error::FFmpegError> {
        let result = unsafe { ffmpeg::avcodec_receive_frame(self.as_mut_ctx(), frame) };
        error::convert_ff_result(result)?;
        self.apply_context_to_frame(frame);
        Ok(())
    }

    fn apply_context_to_frame(&self, frame: &mut Self::Frame) {
        // Apply context to frame, not entirely sure why the filters do not transfer this.
        let ctx = self.as_ctx();
        frame.time_base = ctx.time_base;
    }
}

impl Drop for BaseDecoder {
    fn drop(&mut self) {
        if self.ctx.is_null() {
            return;
        }
        unsafe { ffmpeg::avcodec_free_context(&raw mut self.ctx) };
    }
}
