use std::ffi::CString;
use std::fmt::Formatter;
use std::path::Path;
use std::ptr;
use std::str::FromStr;
use std::time::Duration;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::error;

/// The input source is a media source containing video or audio or both.
///
/// Internally this wraps ffmpeg's audio input system, so any format supported
/// by ffmpeg should be supported by this source.
pub struct InputSource {
    url: url::Url,
    ctx: ptr::NonNull<ffmpeg::AVFormatContext>,
}

impl std::fmt::Debug for InputSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "InputSource(url={})", self.url.as_str())
    }
}

impl InputSource {
    /// Creates a new [InputSource] using the file path to the given media.
    ///
    /// This is a helper method around [InputSource::open_url] and will
    /// convert the file path into a valid URL for FFmpeg to open.
    pub fn open_file(path: impl AsRef<Path>) -> crate::Result<Self> {
        let resolved_path = path
            .as_ref()
            .canonicalize()
            .expect("canonicalize should not fail in normal circumstances");
        let url = format!("file://{}", resolved_path.display())
            .parse()
            .expect("url parses should not fail");
        Self::open_url(url)
    }

    /// Create a new [InputSource] using the provided [url::Url].
    ///
    /// The url can be things like local files, HTTP streams, HLS streams, etc...
    ///
    /// WARNING:
    /// This method can block for an arbitrary amount of time as FFmpeg reads the source,
    /// some things like HLS streams can take several seconds.
    pub fn open_url(url: url::Url) -> crate::Result<Self> {
        let url_cstr = CString::from_str(url.as_str())
            .expect("provided URL should never reasonably contain a null terminator mid string");

        let mut ctx = ptr::null_mut();
        let result = unsafe {
             ffmpeg::avformat_open_input(
                &raw mut ctx,
                url_cstr.as_ptr(),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        error::convert_ff_result(result)?;

        if let Some(ctx) = ptr::NonNull::new(ctx) {
            let mut source = Self { url, ctx };
            source.init_source()?;
            Ok(source)
        } else {
            panic!("ffmpeg::avformat_open_input returned null after returning a successful result code");
        }
    }

    fn init_source(&mut self) -> crate::Result<()> {
        let result = unsafe { ffmpeg::avformat_find_stream_info(self.ctx.as_ptr(), ptr::null_mut()) };
        error::convert_ff_result(result)?;
        Ok(())
    }

    /// Returns the duration of the source.
    pub fn duration(&self) -> Duration {
        let ptr = self.ctx.as_ptr();
        let duration = unsafe { (*ptr).duration };
        Duration::from_secs_f32((duration as f64 / ffmpeg::AV_TIME_BASE as f64) as f32)
    }

    /// Returns a reference to the source's URL.
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Returns the number of streams available from the source.
    pub fn num_streams(&self) -> usize {
        let ptr = self.ctx.as_ptr();
        unsafe { (*ptr).nb_streams as usize }
    }


}

// SAFETY: We are allowed to call `avformat_free_context` from a different thread to
//         where we called `avformat_open_input`.
unsafe impl Send for InputSource {}

impl Drop for InputSource {
    fn drop(&mut self) {
        unsafe { ffmpeg::avformat_free_context(self.ctx.as_ptr()) }
    }
}


#[cfg(test)]
mod tests {
    use url::Url;
    use super::*;

    #[test]
    fn test_direct_file_open() {
        let source = InputSource::open_file("../media/test.mp4").unwrap();
        println!("got source: {:?}", source);
        println!("duration: {:?}", source.duration());
        drop(source);
    }
}