use std::time::Duration;

use crate::codec::{BaseDecoder, VideoDecoder};
use crate::{AcceleratorConfig, InputSource, MediaType, OutputPixelFormat, error};

/// The builder for creating new [MediaPlayer] state machines.
pub struct MediaPlayerBuilder {
    source: InputSource,
    target_pixel_formats: Vec<OutputPixelFormat>,
    accelerator_config: AcceleratorConfig,
    stream_index_video: Option<usize>,
    stream_index_audio: Option<usize>,
    stream_index_subtitle: Option<usize>,
}

impl MediaPlayerBuilder {
    /// Create a new [MediaPlayerBuilder] using the given [InputSource]
    /// and safe defaults for all other options.
    pub fn for_source(source: InputSource) -> Self {
        Self {
            source,
            target_pixel_formats: vec![OutputPixelFormat::Nv12],
            accelerator_config: AcceleratorConfig::default(),
            stream_index_video: None,
            stream_index_audio: None,
            stream_index_subtitle: None,
        }
    }

    /// Set the output pixel formats to decode the video into.
    ///
    /// Multiple formats can be specified and the decoder will choose
    /// the pixel format that involves the least amount of work (more or less.)
    ///
    /// This means if your hardware decoder is processing the frame in `p010le`,
    /// it will not convert it into something like `nv12` which involves more
    /// processing or fallback back to software process in some situations
    /// for certain encoders (like Apple's Videotoolbox.)
    pub fn with_target_pixel_formats(
        mut self,
        formats: impl AsRef<[OutputPixelFormat]>,
    ) -> Self {
        self.target_pixel_formats = formats.as_ref().to_vec();
        assert!(
            !self.target_pixel_formats.is_empty(),
            "target pixel formats cannot be empty"
        );
        self.target_pixel_formats.dedup();
        self
    }

    /// Set a custom [AcceleratorConfig] which determines the priority and selection
    /// of hardware decoders used.
    pub fn with_accelerator_config(
        mut self,
        accelerator_config: AcceleratorConfig,
    ) -> Self {
        self.accelerator_config = accelerator_config;
        self
    }

    /// Select a specific video stream to output.
    pub fn with_video_stream(mut self, stream_index: Option<usize>) -> Self {
        if let Some(index) = stream_index {
            let stream = self.source.stream(index);
            assert_eq!(
                stream.media_type,
                MediaType::Video,
                "stream specified is not a video stream"
            );
        }
        self.stream_index_video = stream_index;
        self
    }

    /// Select a specific audio stream to output.
    pub fn with_audio_stream(mut self, stream_index: Option<usize>) -> Self {
        if let Some(index) = stream_index {
            let stream = self.source.stream(index);
            assert_eq!(
                stream.media_type,
                MediaType::Audio,
                "stream specified is not a audio stream"
            );
        }
        self.stream_index_audio = stream_index;
        self
    }

    /// Select a specific subtitle stream to output.
    pub fn with_subtitle_stream(mut self, stream_index: Option<usize>) -> Self {
        if let Some(index) = stream_index {
            let stream = self.source.stream(index);
            assert_eq!(
                stream.media_type,
                MediaType::Subtitle,
                "stream specified is not a subtitle stream"
            );
        }
        self.stream_index_subtitle = stream_index;
        self
    }

    /// Create the [MediaPlayer] using the set config.
    pub fn build(mut self) -> crate::Result<MediaPlayer> {
        let video_stream = self
            .source
            .find_best_stream(MediaType::Video, self.stream_index_video)?;
        let audio_stream = self
            .source
            .find_best_stream(MediaType::Audio, self.stream_index_audio)?;
        let subtitle_stream = self
            .source
            .find_best_stream(MediaType::Subtitle, self.stream_index_subtitle)?;

        if video_stream.is_none() && audio_stream.is_none() && subtitle_stream.is_none()
        {
            return Err(error::PlayerError::NoAvailableStreams);
        }

        tracing::info!(
            video = ?video_stream,
            audio = ?audio_stream,
            subtitle = ?subtitle_stream,
            "setting up player",
        );

        let decoder_video = video_stream
            .as_ref()
            .map(|stream| {
                self.source
                    .open_video_stream(stream.index, &self.accelerator_config)
            })
            .transpose()?;

        let decoder_audio = audio_stream
            .as_ref()
            .map(|stream| self.source.open_stream(stream.index))
            .transpose()?;

        let decoder_subtitle = subtitle_stream
            .as_ref()
            .map(|stream| self.source.open_stream(stream.index))
            .transpose()?;

        // To avoid doing unnecessary work, discard everything but the data we care about.
        self.source.keep_streams(|stream| {
            Some(stream.index) == video_stream.as_ref().map(|info| info.index)
                && Some(stream.index) == audio_stream.as_ref().map(|info| info.index)
                && Some(stream.index) == subtitle_stream.as_ref().map(|info| info.index)
        });

        Ok(MediaPlayer {
            source: self.source,
            decoder_video,
            decoder_audio,
            decoder_subtitle,
            video_filter: None,
        })
    }
}

/// The media player is a state machine for processing incoming video, audio and
/// subtitles from a [InputSource].
///
/// This player requires polling in a loop in order to drive the decoding and
/// processing of the media, typically you would run this in a loop in another
/// thread that occasionally checks if it should play, pause, seek, etc...
pub struct MediaPlayer {
    source: InputSource,

    decoder_video: Option<VideoDecoder>,
    decoder_audio: Option<BaseDecoder>,
    decoder_subtitle: Option<BaseDecoder>,

    video_filter: Option<()>,
}

impl MediaPlayer {
    /// Seek to a target position in the [InputSource].
    pub fn seek(&mut self, position: Duration) -> crate::Result<()> {
        self.source.seek(position)
    }

    /// Begin the media decoding.
    pub fn play(&mut self) -> crate::Result<()> {
        self.source.play()
    }

    /// Pause the media decoding.
    ///
    /// NOTE: This only really applies to network-based streams, if you continue
    /// to poll `process_next_frame` you will continue to get frames.
    pub fn pause(&mut self) -> crate::Result<()> {
        self.source.pause()
    }

    /// Drives the player state machine until at least one frame
    /// is produced or the [InputSource] reaches the end of the stream.
    pub fn process_next_frame(&mut self) -> crate::Result<Option<DecodedFrame>> {
        todo!()
    }
}

/// A frame which has been decoded from the [InputSource].
pub enum DecodedFrame {
    /// A decoded video frame.
    ///
    /// In the pixel format of one of the target [OutputPixelFormat] formats
    /// you configure on the player.
    Video(),
    /// A decoded audio frame.
    Audio(),
    /// A decoded subtitle frame.
    ///
    /// This can be in either text/ASS format or bitmap format.
    Subtitle(),
}
