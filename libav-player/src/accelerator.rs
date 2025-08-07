use rusty_ffmpeg::ffi as ffmpeg;

#[cfg(target_os = "linux")]
/// The default accelerator affinity for a Linux based distribution.
///
/// The order of this API is chosen in order of flexibility, performance
/// and availability of decoders.
static DEFAULT_ACCELERATOR_AFFINITY: &[Accelerator] = &[
    Accelerator::Vaapi,
    Accelerator::Vdpau,
    Accelerator::Cuda,
    Accelerator::Vulkan,
];

#[cfg(target_os = "windows")]
/// The default accelerator affinity for a Windows based distribution.
///
/// The order of this API is chosen in order of flexibility, performance
/// and availability of decoders, in particular, VAAPI is included
/// in this list because it _is_ supported by Windows now but probably
/// is not compiled into FFmpeg for most people.
static DEFAULT_ACCELERATOR_AFFINITY: &[Accelerator] = &[
    Accelerator::Direct3D12,
    Accelerator::Direct3D12,
    Accelerator::Dxva2,
    Accelerator::Cuda,
    Accelerator::Vaapi,
];

#[cfg(target_os = "macos")]
/// The default accelerator affinity for a macOS based distribution.
///
/// This is only video toolbox as that is the only API available really
/// for metal / M-series chips.
static DEFAULT_ACCELERATOR_AFFINITY: &[Accelerator] = &[Accelerator::VideoToolbox];

mod hw_platform_flags {
    pub const WINDOWS: usize = 1 << 0;
    pub const LINUX: usize = 1 << 1;
    pub const MACOS: usize = 1 << 2;
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
/// A hardware accelerator that can be used for encoding or decoding.
pub enum Accelerator {
    /// Video Acceleration API (VAAPI) is a non-proprietary and royalty-free open source
    /// software library ("libva") and API specification, initially developed by Intel but
    /// can be used in combination with other devices.
    ///
    /// It can be used to access the Quick Sync hardware in Intel GPUs and the UVD/VCE
    /// hardware in AMD GPUs.
    Vaapi,
    /// Video Decode and Presentation API for Unix.
    ///
    /// Developed by NVIDIA for Unix/Linux systems.
    /// To enable this you typically need the libvdpau development package in your distribution,
    /// and a compatible graphics card.
    Vdpau,
    /// NVENC and NVDEC are NVIDIA's hardware-accelerated encoding and decoding APIs.
    ///
    /// They used to be called CUVID. They can be used for encoding and decoding on Windows
    /// and Linux. FFmpeg refers to NVENC/NVDEC interconnect as CUDA.
    Cuda,
    /// Intel Quick Sync Video
    ///
    /// This provides some hardware decoders for Intel GPUs on windows or linux.
    Qsv,
    /// Vulkan video decoding is a new specification for vendor-generic hardware accelerated video
    /// decoding.
    ///
    /// Currently, the following codecs are supported:
    /// `H.264`, `HEVC`, `AV1*`
    /// * AV1 format is experimental as of FFmpeg 7.x and only really supposed to be used
    ///   on AMD GPUs using the Mesa drivers.
    Vulkan,
    /// Direct-X Video Acceleration API (Direct 3D 9), developed by Microsoft.
    ///
    /// Windows only.
    Dxva2,
    /// Direct-X Video Acceleration API (Direct 3D 11), developed by Microsoft.
    ///
    /// Windows only.
    D3D11,
    /// Direct-X Video Acceleration API (Direct 3D 12), developed by Microsoft.
    ///
    /// Windows only.
    D3D12,
    /// VideoToolbox is the macOS framework for video decoding and encoding.
    ///
    /// MacOS only.
    VideoToolbox,
}

impl Accelerator {
    pub(crate) fn try_from_av_hw_device_type(
        device_type: ffmpeg::AVHWDeviceType,
    ) -> Option<Self> {
        match device_type {
            ffmpeg::AV_HWDEVICE_TYPE_VAAPI => Some(Accelerator::Vaapi),
            ffmpeg::AV_HWDEVICE_TYPE_VDPAU => Some(Accelerator::Vdpau),
            ffmpeg::AV_HWDEVICE_TYPE_CUDA => Some(Accelerator::Cuda),
            ffmpeg::AV_HWDEVICE_TYPE_QSV => Some(Accelerator::Qsv),
            ffmpeg::AV_HWDEVICE_TYPE_VULKAN => Some(Accelerator::Vulkan),
            ffmpeg::AV_HWDEVICE_TYPE_DXVA2 => Some(Accelerator::Dxva2),
            ffmpeg::AV_HWDEVICE_TYPE_D3D11VA => Some(Accelerator::D3D11),
            ffmpeg::AV_HWDEVICE_TYPE_D3D12VA => Some(Accelerator::D3D12),
            ffmpeg::AV_HWDEVICE_TYPE_VIDEOTOOLBOX => Some(Accelerator::VideoToolbox),
            _ => None,
        }
    }

    fn platform_flags(&self) -> usize {
        match self {
            Accelerator::Vaapi => hw_platform_flags::LINUX | hw_platform_flags::WINDOWS,
            Accelerator::Vdpau => hw_platform_flags::LINUX,
            Accelerator::Cuda => hw_platform_flags::LINUX | hw_platform_flags::WINDOWS,
            Accelerator::Qsv => hw_platform_flags::LINUX | hw_platform_flags::WINDOWS,
            Accelerator::Vulkan => hw_platform_flags::LINUX | hw_platform_flags::WINDOWS,
            Accelerator::Dxva2 => hw_platform_flags::WINDOWS,
            Accelerator::D3D11 => hw_platform_flags::WINDOWS,
            Accelerator::D3D12 => hw_platform_flags::WINDOWS,
            Accelerator::VideoToolbox => hw_platform_flags::MACOS,
        }
    }

    pub(crate) fn to_name(&self) -> &'static str {
        match self {
            Accelerator::Vaapi => "vaapi",
            Accelerator::Vdpau => "vdpau",
            Accelerator::Cuda => "cuda",
            Accelerator::Qsv => "qsv",
            Accelerator::Vulkan => "vulkan",
            Accelerator::Dxva2 => "dxva2",
            Accelerator::D3D11 => "d3d11",
            Accelerator::D3D12 => "d3d12",
            Accelerator::VideoToolbox => "videotoolbox",
        }
    }

    pub(crate) fn to_pixel_format_callback(
        &self,
    ) -> extern "C" fn(
        *mut ffmpeg::AVCodecContext,
        *const ffmpeg::AVPixelFormat,
    ) -> ffmpeg::AVPixelFormat {
        match self {
            Accelerator::Vaapi => select_vaapi_pix_fmt,
            Accelerator::Vdpau => select_vdpau_pix_fmt,
            Accelerator::Cuda => select_cuda_pix_fmt,
            Accelerator::Qsv => select_qsv_pix_fmt,
            Accelerator::Vulkan => select_vulkan_pix_fmt,
            Accelerator::Dxva2 => select_dxva2_pix_fmt,
            Accelerator::D3D11 => select_d3d11_pix_fmt,
            Accelerator::D3D12 => select_d3d12_pix_fmt,
            Accelerator::VideoToolbox => select_videotoolbox_pix_fmt,
        }
    }
}

#[derive(Debug, Clone)]
/// The accelerator config controls the behaviour of hardware decoding used by FFmpeg
/// when processing video streams.
pub struct AcceleratorConfig {
    affinity: Box<[Accelerator]>,
    target_device: Option<std::ffi::CString>,
}

impl Default for AcceleratorConfig {
    fn default() -> Self {
        let mut config = Self {
            affinity: Box::new([]),
            target_device: None,
        };
        config.set_accelerators(DEFAULT_ACCELERATOR_AFFINITY);
        config
    }
}

impl AcceleratorConfig {
    #[inline]
    /// Returns the enabled accelerators in
    /// the order of the affinity for each accelerator.
    pub fn accelerators(&self) -> &[Accelerator] {
        &self.affinity
    }

    pub(crate) fn device_target(&self) -> Option<&std::ffi::CStr> {
        self.target_device.as_deref()
    }

    /// Set the enabled accelerators.
    ///
    /// Order of accelerators matters here as it describes the affinity
    /// the system should have for each accelerator meaning it will pick
    /// hardware decoded in the order defined by this list.
    pub fn set_accelerators(&mut self, accelerators: &[Accelerator]) {
        let target_platform = if cfg!(target_os = "windows") {
            hw_platform_flags::WINDOWS
        } else if cfg!(target_os = "linux") {
            hw_platform_flags::LINUX
        } else if cfg!(target_os = "macos") {
            hw_platform_flags::MACOS
        } else {
            0
        };

        let mut warn_missing_hw_accel = target_platform != 0;
        for accelerator in accelerators {
            if accelerator.platform_flags() & target_platform != 0 {
                warn_missing_hw_accel = false;
                break;
            }
        }

        if warn_missing_hw_accel {
            tracing::warn!(
                accelerators = ?accelerators,
                "current target platform has no hardware accelerators to target, \
                only software decoding is available",
            );
        }

        let mut accelerators_owned = accelerators.to_vec();
        accelerators_owned.dedup();
        self.affinity = accelerators_owned.into_boxed_slice();
    }

    /// Set the target device of the accelerators.
    ///
    /// This allows you to select your discrete GPU vs integrated GPU for example.
    pub fn set_device(&mut self, device: &str) {
        let device_owned = std::ffi::CString::new(device)
            .expect("device string should not contain null terminators");
        self.target_device = Some(device_owned);
    }
}

macro_rules! define_pix_fmt_selector {
    ($name:ident, $target:expr) => {
        extern "C" fn $name(
            _ctx: *mut ffmpeg::AVCodecContext,
            mut pix_fmts: *const ffmpeg::AVPixelFormat,
        ) -> ffmpeg::AVPixelFormat {
            loop {
                let raw_pix_fmt = unsafe { *pix_fmts };
                if raw_pix_fmt == ffmpeg::AV_PIX_FMT_NONE {
                    break;
                } else if raw_pix_fmt == $target {
                    return raw_pix_fmt;
                }

                pix_fmts = unsafe { pix_fmts.offset(1) };
            }

            ffmpeg::AV_PIX_FMT_NONE
        }
    };
}

define_pix_fmt_selector!(select_vaapi_pix_fmt, ffmpeg::AV_PIX_FMT_VAAPI);
define_pix_fmt_selector!(select_vdpau_pix_fmt, ffmpeg::AV_PIX_FMT_VDPAU);
define_pix_fmt_selector!(select_cuda_pix_fmt, ffmpeg::AV_PIX_FMT_CUDA);
define_pix_fmt_selector!(select_qsv_pix_fmt, ffmpeg::AV_PIX_FMT_QSV);
define_pix_fmt_selector!(select_vulkan_pix_fmt, ffmpeg::AV_PIX_FMT_VULKAN);
define_pix_fmt_selector!(select_dxva2_pix_fmt, ffmpeg::AV_PIX_FMT_DXVA2_VLD);
define_pix_fmt_selector!(select_d3d11_pix_fmt, ffmpeg::AV_PIX_FMT_D3D11);
define_pix_fmt_selector!(select_d3d12_pix_fmt, ffmpeg::AV_PIX_FMT_D3D12);
define_pix_fmt_selector!(select_videotoolbox_pix_fmt, ffmpeg::AV_PIX_FMT_VIDEOTOOLBOX);
