use std::ptr;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::accelerator::{Accelerator, AcceleratorConfig};
use crate::error;

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
    /// Create a new decoder using the target codec.
    fn create(codec: &'static ffmpeg::AVCodec) -> Result<Self, error::FFmpegError>;

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

    /// Push packet data into the decoder.
    fn write_packet(
        &mut self,
        packet: &mut ffmpeg::AVPacket,
    ) -> Result<(), error::FFmpegError> {
        let result = unsafe { ffmpeg::avcodec_send_packet(self.as_mut_ctx(), packet) };
        error::convert_ff_result(result)?;
        Ok(())
    }

    /// Attempt to decode a new frame and write it to the provided [ffmpeg::AVFrame].
    fn decode(&mut self, frame: &mut ffmpeg::AVFrame) -> Result<(), error::FFmpegError> {
        let result = unsafe { ffmpeg::avcodec_receive_frame(self.as_mut_ctx(), frame) };
        error::convert_ff_result(result)?;
        Ok(())
    }
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
        codec_params: Option<&ffmpeg::AVCodecParameters>,
    ) -> Result<Self, error::FFmpegError> {
        let mut decoder = Self::create(codec)?;
        if let Some(codec_params) = codec_params {
            decoder.copy_codec_params(codec_params)?;
        }
        decoder.open()?;
        Ok(decoder)
    }
}

impl Decoder for BaseDecoder {
    fn create(codec: &'static ffmpeg::AVCodec) -> Result<Self, error::FFmpegError> {
        let context = unsafe { ffmpeg::avcodec_alloc_context3(codec) };
        if context.is_null() {
            return Err(error::FFmpegError::custom(
                "failed to allocate codec context",
            ));
        }

        tracing::debug!("created decoder");

        Ok(Self {
            ctx: context,
            codec,
            is_open: false,
        })
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
}

impl Drop for BaseDecoder {
    fn drop(&mut self) {
        if self.ctx.is_null() {
            return;
        }
        unsafe { ffmpeg::avcodec_free_context(&raw mut self.ctx) };
    }
}

/// The accelerated codec is a wrapper around [ffmpeg::AVCodec]
/// and some hardware device if available.
///
/// The codec is must have the stream codec parameters copied across
/// and opened before it can be used.
pub(crate) struct VideoDecoder {
    base_decoder: BaseDecoder,
    accelerator: Option<Accelerator>,
}

impl VideoDecoder {
    /// Open the video decoder.
    ///
    /// This will automatically attempt to use hardware acceleration in the order defined by the
    /// [AcceleratorConfig] and use the first accelerator that supports the codec and target pixel
    /// format output.
    /// If no hardware accelerator is available this will fall back to software.
    ///
    /// The decoder is automatically opened and ready once returned.
    pub(crate) fn open(
        codec: &'static ffmpeg::AVCodec,
        codec_params: Option<&ffmpeg::AVCodecParameters>,
        accelerator_config: &AcceleratorConfig,
    ) -> Result<Self, error::FFmpegError> {
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

            if let Some(codec_params) = codec_params {
                decoder.copy_codec_params(codec_params)?;
            }
            decoder.open()?;
            return Ok(decoder);
        }

        let mut decoder = Self::create(codec)?;
        if let Some(codec_params) = codec_params {
            decoder.copy_codec_params(codec_params)?;
        }
        decoder.open()?;

        Ok(decoder)
    }

    pub(crate) fn accelerator(&self) -> Option<Accelerator> {
        self.accelerator
    }

    pub(crate) fn pix_fmt(&self) -> ffmpeg::AVPixelFormat {
        let accelerator = match self.accelerator() {
            None => {
                let ctx = self.as_ctx();
                return ctx.sw_pix_fmt;
            },
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

    // pub(crate) fn filter_input_args(&self) -> std::ffi::CString {
    //     use std::fmt::Write;
    //     let ctx = self.as_ctx();
    //     let mut args = String::new();
    //     write!(args, "width={}", ctx.width).unwrap();
    //     write!(args, ":height={}", ctx.height).unwrap();
    //     write!(args, ":pix_fmt={}", self.pix_fmt()).unwrap();
    //     // TODO: This isn't technically correct, but I am not sure why this is needed or if it
    //     //       is actually used at all?
    //     write!(
    //         args,
    //         ":time_base={}/{}",
    //         ctx.framerate.den, ctx.framerate.num
    //     )
    //         .unwrap();
    //     write!(
    //         args,
    //         ":frame_rate={}/{}",
    //         ctx.framerate.num, ctx.framerate.den
    //     )
    //         .unwrap();
    //     write!(args, ":colorspace={}", ctx.colorspace).unwrap();
    //     write!(args, ":range={}", ctx.color_range).unwrap();
    //     write!(
    //         args,
    //         ":pixel_aspect={}/{}",
    //         ctx.sample_aspect_ratio.num, ctx.sample_aspect_ratio.den
    //     )
    //         .unwrap();
    //     tracing::debug!(args = ?args, "got filter args");
    //     std::ffi::CString::new(args).unwrap()
    // }
}

impl Decoder for VideoDecoder {
    fn create(codec: &'static ffmpeg::AVCodec) -> Result<Self, error::FFmpegError> {
        let base_decoder = BaseDecoder::create(codec)?;
        Ok(Self {
            base_decoder,
            accelerator: None,
        })
    }

    fn as_mut_ctx(&mut self) -> &mut ffmpeg::AVCodecContext {
        self.base_decoder.as_mut_ctx()
    }

    fn as_ctx(&self) -> &ffmpeg::AVCodecContext {
        self.base_decoder.as_ctx()
    }

    fn open(&mut self) -> Result<(), error::FFmpegError> {
        // Open should never normally be called twice.
        if self.is_open() {
            panic!("codec is already open");
        }

        let target_accelerator = self.accelerator();

        let ctx = self.as_mut_ctx();
        if let Some(accelerator) = target_accelerator {
            ctx.get_format = Some(accelerator.to_pixel_format_callback());
        }

        let result = unsafe {
            ffmpeg::avcodec_open2(ctx, self.base_decoder.codec, ptr::null_mut())
        };
        error::convert_ff_result(result)?;

        Ok(())
    }
}

/// Attempts to create the codec with the given accelerator.
///
/// Returns `None` if the accelerator is not available for the given codec
/// or not available at all.
fn create_accelerated_decoder(
    mut codec: &'static ffmpeg::AVCodec,
    target_accelerator: Accelerator,
    target_device: Option<&std::ffi::CStr>,
) -> Result<Option<VideoDecoder>, error::FFmpegError> {
    let hw_config = find_accelerator_config(codec, target_accelerator);
    if hw_config.is_null() {
        let full_codec_name =
            format_codec_name_with_accelerator(codec, target_accelerator);
        let decoder = find_decoder_by_name(&full_codec_name);
        if let Some(decoder) = decoder {
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

            let ctx = codec.as_mut_ctx();
            ctx.hw_device_ctx = ffmpeg::av_buffer_ref(hw_device);
        };

        codec.accelerator = Some(target_accelerator);
    }

    Ok(Some(codec))
}

fn format_codec_name_with_accelerator(
    codec: &ffmpeg::AVCodec,
    accelerator: Accelerator,
) -> String {
    let codec_name_raw = unsafe { std::ffi::CStr::from_ptr(codec.name) };
    let codec_name = codec_name_raw.to_string_lossy();
    format!("{codec_name}_{}", accelerator.to_name())
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
        let codec = find_decoder_by_name("h264").unwrap();
        let output = format_codec_name_with_accelerator(codec, Accelerator::Vaapi);
        assert_eq!(output, "h264_vaapi");
    }

    #[rstest::rstest]
    #[case::h264_nv12("h264")]
    #[case::hevc_nv12("hevc")]
    #[case::av1_nv12("av1")]
    fn test_create_video_decoder(#[case] codec_name: &str) {
        let _ = tracing_subscriber::fmt::try_init();

        let codec = find_decoder_by_name(codec_name).unwrap();
        let config = AcceleratorConfig::default();
        let video_decoder = VideoDecoder::open(codec, None, &config)
            .expect("accelerated codec creation failed");
        assert!(video_decoder.is_open());
        drop(video_decoder);
    }
}
