
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
    /// Vulkan video decoding is a new specification for vendor-generic hardware accelerated video
    /// decoding.
    ///
    /// Currently, the following codecs are supported:
    /// `H.264`, `HEVC`, `AV1*`
    /// * AV1 format is experimental as of FFmpeg 7.x and only really supposed to be used
    ///   on non AMD GPUs using the Mesa drivers.
    Vulkan,
    /// VideoToolbox is the macOS framework for video decoding and encoding.
    ///
    /// MacOS only.
    VideoToolbox
}