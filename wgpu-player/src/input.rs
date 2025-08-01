use std::ffi::CString;
use std::fmt::Formatter;
use std::path::Path;
use std::ptr;
use std::str::FromStr;
use std::time::Duration;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::stream::Stream;
use crate::{MediaType, error};

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
        let url_cstr = CString::from_str(url.as_str()).expect(
            "provided URL should never reasonably contain a null terminator mid string",
        );

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
            panic!(
                "ffmpeg::avformat_open_input returned null after returning a successful result code"
            );
        }
    }

    fn init_source(&mut self) -> crate::Result<()> {
        let result = unsafe {
            ffmpeg::avformat_find_stream_info(self.ctx.as_ptr(), ptr::null_mut())
        };
        error::convert_ff_result(result)?;
        Ok(())
    }

    /// Returns the duration of the source.
    pub fn duration(&self) -> Duration {
        let ctx = self.ctx.as_ptr();
        let duration = unsafe { (*ctx).duration };
        Duration::from_secs_f32(duration as f32 / ffmpeg::AV_TIME_BASE as f32)
    }

    /// Returns a reference to the source's URL.
    pub fn url(&self) -> &url::Url {
        &self.url
    }

    /// Returns the number of streams available from the source.
    pub fn num_streams(&self) -> usize {
        let ctx = self.ctx.as_ptr();
        unsafe { (*ctx).nb_streams as usize }
    }

    /// Iterate over all available audio, video and subtitle streams in the source.
    pub fn iter_streams(&self) -> impl Iterator<Item = Stream> {
        let ctx = self.ctx.as_ptr();
        let streams =
            unsafe { std::slice::from_raw_parts((*ctx).streams, self.num_streams()) };

        streams
            .iter()
            .map(|v| unsafe { Stream::from_raw(*v) })
            .filter(|stream| {
                matches!(
                    stream.media_type(),
                    MediaType::Video | MediaType::Audio | MediaType::Subtitle
                )
            })
    }

    /// Find the best stream for the given [MediaType].
    ///
    /// An optional `preferred_stream_index` can be provided
    pub fn find_best_stream(
        &self,
        media_type: MediaType,
        preferred_stream_index: Option<usize>,
    ) -> crate::Result<Option<Stream>> {
        let ctx = self.ctx.as_ptr();

        let mut decoder = ptr::null();
        let result = unsafe {
            ffmpeg::av_find_best_stream(
                ctx,
                media_type.to_av_media_type(),
                preferred_stream_index.map(|v| v as i32).unwrap_or(-1),
                -1,
                &raw mut decoder,
                0,
            )
        };

        let result = error::convert_ff_result(result);
        let stream_index = match result {
            Ok(index) => index as usize,
            Err(err) if err.errno() == ffmpeg::AVERROR_STREAM_NOT_FOUND => {
                return Ok(None);
            },
            Err(other) => return Err(other.into()),
        };

        let stream = unsafe {
            let streams = std::slice::from_raw_parts((*ctx).streams, self.num_streams());
            Stream::from_raw(streams[stream_index])
        };

        Ok(Some(stream))
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
    use super::*;
    use crate::stream::{FrameRate, Resolution};

    #[test]
    fn test_direct_file_open() {
        let source = InputSource::open_file("../media/test.mp4").unwrap();
        assert_eq!(source.num_streams(), 2);
        assert_eq!(source.duration(), Duration::from_secs_f32(13.845000267));
    }

    #[test]
    fn test_iter_streams() {
        let source = InputSource::open_file("../media/test.mp4").unwrap();
        let streams = source.iter_streams();

        let mut video_count = 0;
        let mut audio_count = 0;
        let mut subtitles_count = 0;
        for stream in streams {
            if stream.media_type() == MediaType::Video {
                video_count += 1;
            } else if stream.media_type() == MediaType::Audio {
                audio_count += 1;
            } else if stream.media_type() == MediaType::Subtitle {
                subtitles_count += 1;
            }
        }

        assert_eq!(video_count, 1);
        assert_eq!(audio_count, 1);
        assert_eq!(subtitles_count, 0);
    }

    #[test]
    fn test_find_best_stream() {
        let source = InputSource::open_file("../media/test.mp4").unwrap();

        let stream = source
            .find_best_stream(MediaType::Video, None)
            .expect("video stream exists with known decoder")
            .expect("video stream exists");
        assert_eq!(stream.index(), 0);
        assert_eq!(stream.codec_name(), "h264");
        assert_eq!(stream.bitrate(), Some(5160));
        assert_eq!(
            stream.resolution(),
            Some(Resolution {
                width: 1920,
                height: 1080
            })
        );
        assert_eq!(stream.framerate(), FrameRate::new(25, 1));
        assert_eq!(stream.media_type(), MediaType::Video);

        let stream = source
            .find_best_stream(MediaType::Audio, None)
            .expect("audio stream exists with known decoder")
            .expect("audio stream exists");
        assert_eq!(stream.index(), 1);
        assert_eq!(stream.codec_name(), "aac");
        assert_eq!(stream.bitrate(), Some(253));
        assert_eq!(stream.framerate(), FrameRate::new(0, 0));
        assert_eq!(stream.media_type(), MediaType::Audio);

        let stream = source
            .find_best_stream(MediaType::Subtitle, None)
            .expect("call should succeed despite no stream existing");
        assert!(stream.is_none(), "no subtitle stream should exist");

        let stream = source
            .find_best_stream(MediaType::Video, Some(2))
            .expect("call should succeed despite no stream existing");
        assert!(
            stream.is_none(),
            "no video stream should exist at user provided index"
        );
    }
}
