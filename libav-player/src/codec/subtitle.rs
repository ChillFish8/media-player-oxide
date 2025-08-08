use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::codec::{BaseDecoder, Decoder};
use crate::error;
use crate::stream::StreamInfo;

/// A decoder for processing subtitle streams.
pub struct SubtitleDecoder {
    inner: BaseDecoder,

    ready_subtitle: Option<ffmpeg::AVSubtitle>,
}

impl SubtitleDecoder {
    pub(crate) fn open(
        codec: &'static ffmpeg::AVCodec,
        stream_info: StreamInfo,
        codec_params: Option<&ffmpeg::AVCodecParameters>,
    ) -> Result<Self, error::FFmpegError> {
        let inner = BaseDecoder::open(codec, stream_info, codec_params)?;
        Ok(Self {
            inner,
            ready_subtitle: None,
        })
    }
}

impl Decoder for SubtitleDecoder {
    type Frame = ffmpeg::AVSubtitle;

    fn as_mut_ctx(&mut self) -> &mut ffmpeg::AVCodecContext {
        self.inner.as_mut_ctx()
    }

    fn as_ctx(&self) -> &ffmpeg::AVCodecContext {
        self.inner.as_ctx()
    }

    fn open(&mut self) -> Result<(), error::FFmpegError> {
        self.inner.open()
    }

    fn write_packet(&mut self, packet: &mut ffmpeg::AVPacket) -> Result<(), error::FFmpegError> {
        let mut subtitle = ffmpeg::AVSubtitle {
            format: 0,
            start_display_time: 0,
            end_display_time: 0,
            num_rects: 0,
            rects: ptr::null_mut(),
            pts: 0,
        };

        let mut got_sub_ptr = 0;

        let result = unsafe {
            ffmpeg::avcodec_decode_subtitle2(
                self.as_mut_ctx(),
                &raw mut subtitle,
                &raw mut got_sub_ptr,
                packet,
            )
        };
        error::convert_ff_result(result)?;

        if got_sub_ptr != 0 {
            debug_assert!(
                self.ready_subtitle.is_none(),
                "state machine behaviour is incorrect if a ready subtitle is being overwritten",
            );
            self.ready_subtitle = Some(subtitle);
        }

        Ok(())
    }

    fn decode(&mut self, frame: &mut Self::Frame) -> Result<(), error::FFmpegError> {
        let ready = self.ready_subtitle
            .take()
            .ok_or_else(|| error::FFmpegError::from_raw_errno(-(ffmpeg::EAGAIN as i32)))?;
        *frame = ready;
        Ok(())
    }

    fn apply_context_to_frame(&self, _frame: &mut Self::Frame) {

    }
}
