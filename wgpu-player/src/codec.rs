use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::accelerator::{Accelerator, AcceleratorConfig};
use crate::error;

/// Open the video decoder.
///
/// This will automatically attempt to use hardware acceleration in the order defined by the
/// [AcceleratorConfig] and use the first accelerator that supports the codec and target pixel
/// format output.
/// If no hardware accelerator is available this will fall back to software.
///
/// The decoder is automatically opened and ready once returned.
pub(crate) fn open_best_fitting_decoder(
    codec: *const ffmpeg::AVCodec,
    codec_params: *const ffmpeg::AVCodecParameters,
    accelerator_config: &AcceleratorConfig,
) -> Result<VideoDecoder, error::FFmpegError> {
    for accelerator in accelerator_config.accelerators() {
        tracing::debug!(accelerator = ?accelerator, "attempting to use accelerator");

        let result = create_accelerated_decoder(
            codec,
            *accelerator,
            accelerator_config.device_target(),
        );

        let mut decoder = match result {
            Ok(Some(decoder)) => decoder,
            Ok(None) => continue,
            Err(err) => return Err(err),
        };

        tracing::debug!(accelerator = ?accelerator, "accelerator exists");

        decoder.copy_codec_params(codec_params)?;

        decoder.open()?;
        return Ok(decoder);
    }

    let mut default_decoder = VideoDecoder::create(codec)?;
    default_decoder.copy_codec_params(codec_params)?;
    default_decoder.open()?;

    Ok(default_decoder)
}

/// Attempts to create the codec with the given accelerator.
///
/// Returns `None` if the accelerator is not available for the given codec
/// or not available at all.
fn create_accelerated_decoder(
    mut codec: *const ffmpeg::AVCodec,
    target_accelerator: Accelerator,
    target_device: Option<&std::ffi::CStr>,
) -> Result<Option<VideoDecoder>, error::FFmpegError> {
    let hw_config = find_accelerator_config(codec, target_accelerator);
    if hw_config.is_null() {
        let full_codec_name =
            format_codec_name_with_accelerator(codec, target_accelerator);
        let decoder =
            unsafe { ffmpeg::avcodec_find_decoder_by_name(full_codec_name.as_ptr()) };
        if !decoder.is_null() {
            codec = decoder;
        } else {
            return Ok(None);
        }
    }

    let mut codec = VideoDecoder::create(codec)?;
    let mut hw_device = ptr::null_mut();

    if !hw_config.is_null() {
        unsafe {
            let result = ffmpeg::av_hwdevice_ctx_create(
                &raw mut hw_device,
                (*hw_config).device_type,
                target_device
                    .map(|device| device.as_ptr())
                    .unwrap_or(ptr::null()),
                ptr::null_mut(),
                0,
            );
            error::convert_ff_result(result)?;

            (*codec.ctx).hw_device_ctx = ffmpeg::av_buffer_ref(hw_device);
        };

        codec.accelerator = Some(target_accelerator);
    }

    Ok(Some(codec))
}

/// The accelerated codec is a wrapper around [ffmpeg::AVCodec]
/// and some hardware device if available.
///
/// The codec is must have the stream codec parameters copied across
/// and opened before it can be used.
pub(crate) struct VideoDecoder {
    ctx: *mut ffmpeg::AVCodecContext,
    codec: *const ffmpeg::AVCodec,
    is_open: bool,
    accelerator: Option<Accelerator>,
}

impl VideoDecoder {
    fn create(codec: *const ffmpeg::AVCodec) -> Result<Self, error::FFmpegError> {
        let context = unsafe { ffmpeg::avcodec_alloc_context3(codec) };
        if context.is_null() {
            return Err(error::FFmpegError::custom(
                "failed to allocate codec context",
            ));
        }

        tracing::debug!("creating video decoder");

        let codec = VideoDecoder {
            ctx: context,
            codec,
            is_open: false,
            accelerator: None,
        };

        Ok(codec)
    }

    pub(crate) fn accelerator(&self) -> Option<Accelerator> {
        self.accelerator
    }

    pub(crate) fn hw_device_ctx(&self) -> *mut ffmpeg::AVBufferRef {
        unsafe { (*self.ctx).hw_device_ctx }
    }

    pub(crate) fn hw_frame_ctx(&self) -> *mut ffmpeg::AVBufferRef {
        unsafe { (*self.ctx).hw_frames_ctx }
    }

    fn pix_fmt(&self) -> ffmpeg::AVPixelFormat {
        let accelerator = match self.accelerator() {
            None => return unsafe { (*self.ctx).sw_pix_fmt },
            Some(accelerator) => accelerator,
        };

        match accelerator {
            Accelerator::Vaapi => ffmpeg::AV_PIX_FMT_VAAPI,
            Accelerator::Vdpau => ffmpeg::AV_PIX_FMT_VDPAU,
            Accelerator::Cuda => ffmpeg::AV_PIX_FMT_CUDA,
            Accelerator::Qsv => ffmpeg::AV_PIX_FMT_QSV,
            Accelerator::Vulkan => ffmpeg::AV_PIX_FMT_VULKAN,
            Accelerator::Dxva2 => ffmpeg::AV_PIX_FMT_DXVA2_VLD,
            Accelerator::D3D11 => ffmpeg::AV_PIX_FMT_D3D11,
            Accelerator::D3D12 => ffmpeg::AV_PIX_FMT_D3D12,
            Accelerator::VideoToolbox => ffmpeg::AV_PIX_FMT_VIDEOTOOLBOX,
        }
    }

    pub(crate) fn filter_input_args(&self) -> std::ffi::CString {
        use std::fmt::Write;
        let ctx = unsafe { &*self.ctx };
        let mut args = String::new();
        write!(args, "width={}", ctx.width).unwrap();
        write!(args, ":height={}", ctx.height).unwrap();
        write!(args, ":pix_fmt={}", self.pix_fmt()).unwrap();
        // TODO: This isn't technically correct, but I am not sure why this is needed or if it
        //       is actually used at all?
        write!(
            args,
            ":time_base={}/{}",
            ctx.framerate.den, ctx.framerate.num
        )
        .unwrap();
        write!(
            args,
            ":frame_rate={}/{}",
            ctx.framerate.num, ctx.framerate.den
        )
        .unwrap();
        write!(args, ":colorspace={}", ctx.colorspace).unwrap();
        write!(args, ":range={}", ctx.color_range).unwrap();
        write!(
            args,
            ":pixel_aspect={}/{}",
            ctx.sample_aspect_ratio.num, ctx.sample_aspect_ratio.den
        )
        .unwrap();
        tracing::debug!(args = ?args, "got filter args");
        std::ffi::CString::new(args).unwrap()
    }

    fn copy_codec_params(
        &mut self,
        params: *const ffmpeg::AVCodecParameters,
    ) -> Result<(), error::FFmpegError> {
        if params.is_null() {
            return Ok(());
        }

        let result = unsafe { ffmpeg::avcodec_parameters_to_context(self.ctx, params) };
        error::convert_ff_result(result)?;
        Ok(())
    }

    fn open(&mut self) -> Result<(), error::FFmpegError> {
        // Open should never normally be called twice.
        if self.is_open {
            panic!("codec is already open");
        }

        if let Some(accelerator) = self.accelerator() {
            unsafe {
                (*self.ctx).get_format = Some(accelerator.to_pixel_format_callback())
            };
        }

        let result =
            unsafe { ffmpeg::avcodec_open2(self.ctx, self.codec, ptr::null_mut()) };
        error::convert_ff_result(result)?;

        self.is_open = true;

        Ok(())
    }

    pub(crate) fn decode(
        &mut self,
        packet: &mut ffmpeg::AVPacket,
    ) -> Result<(), error::FFmpegError> {
        unsafe {
            let mut frame = ffmpeg::av_frame_alloc();

            let result = ffmpeg::avcodec_send_packet(self.ctx, packet);
            error::convert_ff_result(result)?;

            loop {
                let result = ffmpeg::avcodec_receive_frame(self.ctx, frame);
                if result == ffmpeg::AVERROR_EOF || result == -(ffmpeg::EAGAIN as i32) {
                    ffmpeg::av_frame_free(&raw mut frame);
                    return Ok(());
                }
                error::convert_ff_result(result)?;

                dbg!((*(*frame).hw_frames_ctx).data);
                dbg!((*(*self.ctx).hw_frames_ctx).data);
            }
        }
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        if self.ctx.is_null() {
            return;
        }

        unsafe { ffmpeg::avcodec_free_context(&raw mut self.ctx) };
    }
}

fn format_codec_name_with_accelerator(
    codec: *const ffmpeg::AVCodec,
    accelerator: Accelerator,
) -> std::ffi::CString {
    let codec_name_raw = unsafe { std::ffi::CStr::from_ptr((*codec).name) };
    let codec_name = codec_name_raw.to_string_lossy();
    let full_codec_name = format!("{codec_name}_{}", accelerator.to_name());
    std::ffi::CString::new(full_codec_name)
        .expect("formatted codec should not contain any null terminators")
}

fn find_accelerator_config(
    codec: *const ffmpeg::AVCodec,
    target_accelerator: Accelerator,
) -> *const ffmpeg::AVCodecHWConfig {
    for i in 0.. {
        let config = unsafe { ffmpeg::avcodec_get_hw_config(codec, i) };
        if config.is_null() {
            break;
        }

        let maybe_recognised_accelerator = unsafe {
            let hw_device_type_raw = (*config).device_type;
            Accelerator::try_from_av_hw_device_type(hw_device_type_raw)
        };

        let Some(available_accelerator) = maybe_recognised_accelerator else {
            continue;
        };
        tracing::debug!(accelerator = ?available_accelerator, "available accelerator");

        if available_accelerator == target_accelerator {
            return config;
        }
    }

    ptr::null()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accelerator::AcceleratorConfig;

    #[test]
    fn test_format_codec_name_with_accelerator() {
        let codec = unsafe { ffmpeg::avcodec_find_decoder_by_name(c"h264".as_ptr()) };
        let output = format_codec_name_with_accelerator(codec, Accelerator::Vaapi);
        assert_eq!(output.as_c_str(), c"h264_vaapi");
    }

    #[rstest::rstest]
    #[case::h264_nv12(c"h264")]
    #[case::hevc_nv12(c"hevc")]
    #[case::av1_nv12(c"av1")]
    fn test_create_video_decoder(#[case] codec_name: &std::ffi::CStr) {
        let _ = tracing_subscriber::fmt::try_init();

        let codec = unsafe { ffmpeg::avcodec_find_decoder_by_name(codec_name.as_ptr()) };
        assert!(!codec.is_null());

        let config = AcceleratorConfig::default();
        let video_decoder = open_best_fitting_decoder(codec, ptr::null(), &config)
            .expect("accelerated codec creation failed");
        assert!(video_decoder.is_open);
        drop(video_decoder);
    }
}
