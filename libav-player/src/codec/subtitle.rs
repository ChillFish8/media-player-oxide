use rusty_ffmpeg::ffi as ffmpeg;

use crate::codec::{BaseDecoder, Decoder};
use crate::error;
use crate::stream::StreamInfo;

/// A decoder for processing subtitle streams.
pub struct SubtitleDecoder {
    inner: BaseDecoder,
}

impl SubtitleDecoder {
    pub(crate) fn open(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
        codec_params: Option<&ffmpeg::AVCodecParameters>,
    ) -> Result<Self, error::FFmpegError> {
        let inner = BaseDecoder::open(codec, stream_info, codec_params)?;
        Ok(Self { inner })
    }
}

impl Decoder for SubtitleDecoder {
    type Frame = ffmpeg::AVSubtitle;

    fn create(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
    ) -> Result<Self, error::FFmpegError> {
        todo!()
    }

    fn as_mut_ctx(&mut self) -> &mut ffmpeg::AVCodecContext {
        todo!()
    }

    fn as_ctx(&self) -> &ffmpeg::AVCodecContext {
        todo!()
    }

    fn open(&mut self) -> Result<(), error::FFmpegError> {
        todo!()
    }

    fn decode(&mut self, frame: &mut Self::Frame) -> Result<(), error::FFmpegError> {
        todo!()
    }

    fn apply_context_to_frame(&self, frame: &mut Self::Frame) {
        todo!()
    }
}
