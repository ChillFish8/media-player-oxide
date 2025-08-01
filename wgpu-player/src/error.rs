use rusty_ffmpeg::ffi as ffmpeg;

/// A player result type alias.
pub type Result<T> = std::result::Result<T, PlayerError>;

#[derive(thiserror::Error, Debug)]
/// An error that occurred within the player.
pub enum PlayerError {
    #[error(transparent)]
    /// An error the was raised by the FFmpeg libraries.
    FFmpegError(#[from] FFmpegError),
}

#[derive(Debug)]
/// An error originating from libav / FFmpeg.
pub struct FFmpegError {
    errno: i32,
    msg: String,
}

impl FFmpegError {
    #[inline]
    /// Returns the errno returned from FFmpeg.
    pub fn errno(&self) -> i32 {
        self.errno
    }

    #[inline]
    /// Returns the error message from FFmpeg.
    pub fn message(&self) -> &str {
        &self.msg
    }

    pub(crate) fn from_raw_errno(errno: i32) -> Self {
        let msg = ffmpeg::av_err2str(errno);
        Self { errno, msg }
    }

    pub(crate) fn custom(msg: impl std::fmt::Display) -> Self {
        Self {
            errno: ffmpeg::AVERROR_UNKNOWN,
            msg: msg.to_string(),
        }
    }
}

impl std::fmt::Display for FFmpegError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "FFmpeg Error ({:?}): {}", self.errno, self.msg)
    }
}

impl std::error::Error for FFmpegError {}

#[inline]
/// Converts a FFmpeg result code to a Rust result.
pub(crate) fn convert_ff_result(result: i32) -> std::result::Result<i32, FFmpegError> {
    if result < 0 {
        Err(FFmpegError::from_raw_errno(result))
    } else {
        Ok(result)
    }
}
