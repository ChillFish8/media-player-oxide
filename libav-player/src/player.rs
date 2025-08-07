use std::fmt::Formatter;
use std::mem;
use std::time::Duration;

use rusty_ffmpeg::ffi as ffmpeg;

use crate::codec::{BaseDecoder, Decoder, VideoDecoder};
use crate::stream::StreamInfo;
use crate::{
    AcceleratorConfig,
    InputSource,
    MediaType,
    OutputPixelFormat,
    SampleFormat,
    SubtitleFormat,
    error,
    pts_to_duration,
};

const EAGAIN: i32 = -(ffmpeg::EAGAIN as i32);

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
                let decoder = self.source.open_video_stream(
                    stream.index,
                    &self.accelerator_config,
                    self.target_pixel_formats,
                )?;
                Ok::<_, error::FFmpegError>(TaggedDecoder {
                    stream: stream.clone(),
                    decoder,
                })
            })
            .transpose()?;

        let decoder_audio = audio_stream
            .as_ref()
            .map(|stream| {
                let decoder = self.source.open_stream(stream.index)?;
                Ok::<_, error::FFmpegError>(TaggedDecoder {
                    stream: stream.clone(),
                    decoder,
                })
            })
            .transpose()?;

        let decoder_subtitle = subtitle_stream
            .as_ref()
            .map(|stream| {
                let decoder = self.source.open_stream(stream.index)?;
                Ok::<_, error::FFmpegError>(TaggedDecoder {
                    stream: stream.clone(),
                    decoder,
                })
            })
            .transpose()?;

        // To avoid doing unnecessary work, discard everything but the data we care about.
        self.source.keep_streams(|stream| {
            Some(stream.index) == video_stream.as_ref().map(|info| info.index)
                || Some(stream.index) == audio_stream.as_ref().map(|info| info.index)
                || Some(stream.index) == subtitle_stream.as_ref().map(|info| info.index)
        });

        Ok(MediaPlayer {
            source: self.source,

            decoder_video,
            decoder_audio,
            decoder_subtitle,

            packet: MediaPacket::new()?,
            frame_video: MediaFrame::new()?,
            frame_video_ready: None,
            frame_audio: MediaFrame::new()?,
            frame_audio_ready: None,
            frame_subtitle: MediaFrame::new()?,
            frame_subtitle_ready: None,

            end_of_packet_stream: false,

            statistics: PlayerStatistics::default(),
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

    decoder_video: Option<TaggedDecoder<VideoDecoder>>,
    decoder_audio: Option<TaggedDecoder<BaseDecoder>>,
    decoder_subtitle: Option<TaggedDecoder<BaseDecoder>>,

    packet: MediaPacket,
    /// A frame holding video data.
    frame_video: MediaFrame,
    /// If `Some`, signals if the video frame already has valid data
    /// ready and provides the PTS timestamp which can be used for
    /// priority.
    frame_video_ready: Option<i64>,
    /// A frame holding audio data.
    frame_audio: MediaFrame,
    /// If `Some`, signals if the audio frame already has valid data
    /// ready and provides the PTS timestamp which can be used for
    /// priority.
    frame_audio_ready: Option<i64>,
    /// A frame holding subtitle data.
    frame_subtitle: MediaFrame,
    /// If `Some`, signals if the subtitle frame already has valid data
    /// ready and provides the PTS timestamp which can be used for
    /// priority.
    frame_subtitle_ready: Option<i64>,

    end_of_packet_stream: bool,

    statistics: PlayerStatistics,
}

impl MediaPlayer {
    #[inline]
    /// Returns a read-only view of the current player statistics.
    pub fn statistics(&self) -> &PlayerStatistics {
        &self.statistics
    }

    /// Seek to a target position in the [InputSource].
    pub fn seek(&mut self, position: Duration) -> crate::Result<()> {
        tracing::info!(position = ?position, "seeking playback");
        self.source.seek(position).map_err(error::PlayerError::from)
    }

    /// Begin the media decoding.
    pub fn play(&mut self) -> crate::Result<()> {
        tracing::info!("starting playback");
        if let Err(err) = self.source.play() {
            if err.errno() == -38 {
                Ok(())
            } else {
                Err(err.into())
            }
        } else {
            Ok(())
        }
    }

    /// Pause the media decoding.
    ///
    /// NOTE: This only really applies to network-based streams, if you continue
    /// to poll `process_next_frame` you will continue to get frames.
    pub fn pause(&mut self) -> crate::Result<()> {
        tracing::info!("pausing playback");
        if let Err(err) = self.source.pause() {
            if err.errno() == -38 {
                Ok(())
            } else {
                Err(err.into())
            }
        } else {
            Ok(())
        }
    }

    /// Drives the player state machine until at least one frame
    /// is produced or the [InputSource] reaches the end of the stream.
    pub fn process_next_frame(&mut self) -> crate::Result<DecodedFrame> {
        let start = std::time::Instant::now();
        let frame = loop {
            let result = self.get_next_frame();
            match result {
                Ok(frame) => break frame,
                Err(err) if err.needs_data() || err.is_eof() => {
                    if self.end_of_packet_stream {
                        tracing::debug!("end of stream processes");
                        return Err(error::PlayerError::EndOfStream);
                    }
                },
                Err(err) => return Err(err.into()),
            }

            match self.read_next_packet() {
                Err(err) if err.is_eof() => {
                    tracing::debug!("end of stream packets");
                    self.end_of_packet_stream = true;
                    self.flush()?;
                    continue;
                },
                Err(err) => return Err(err.into()),
                Ok(()) => {},
            };

            self.dispatch_packet()?;
        };
        self.statistics.frames_decoded_total += 1;
        self.statistics.frames_total_time += start.elapsed();
        Ok(frame)
    }

    /// Retrieves the next available frame from the decoders.
    ///
    /// Priority is given to the frames already available and will be
    /// returned in order of their PTS.
    ///
    /// After the already ready frames have been processed, we will poll each
    /// decoder once and update the ready states.
    fn get_next_frame(&mut self) -> Result<DecodedFrame, error::FFmpegError> {
        #[cfg(feature = "trace-hotpath")]
        tracing::trace!("trying to get next frame");

        // TODO: We always allocate a new frame because of lifetimes, can we improve this?
        if let Some(frame) = self.get_ready_frame()? {
            #[cfg(feature = "trace-hotpath")]
            tracing::trace!("using ready frame");
            return Ok(frame);
        }

        let start = std::time::Instant::now();
        if let Some(video) = self.decoder_video.as_mut() {
            let is_ok =
                ignore_out_of_data_error(video.decoder.decode(&mut self.frame_video))?;
            if is_ok {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("video frame is ready");
                self.frame_video_ready = Some(self.frame_video.pts);
                self.statistics.num_video_frames_decoded += 1;
            }
        }

        if let Some(audio) = self.decoder_audio.as_mut() {
            let is_ok =
                ignore_out_of_data_error(audio.decoder.decode(&mut self.frame_audio))?;
            if is_ok {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("audio frame is ready");
                self.frame_audio_ready = Some(self.frame_audio.pts);
                self.statistics.num_audio_frames_decoded += 1;
            }
        }

        if let Some(subtitle) = self.decoder_subtitle.as_mut() {
            let is_ok = ignore_out_of_data_error(
                subtitle.decoder.decode(&mut self.frame_subtitle),
            )?;
            if is_ok {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("subtitle frame is ready");
                self.frame_subtitle_ready = Some(self.frame_subtitle.pts);
                self.statistics.num_subtitle_frames_decoded += 1;
            }
        }
        self.statistics.frames_decoded_time += start.elapsed();

        if let Some(frame) = self.get_ready_frame()? {
            #[cfg(feature = "trace-hotpath")]
            tracing::trace!("using just processed frame");
            Ok(frame)
        } else {
            Err(error::FFmpegError::from_raw_errno(EAGAIN))
        }
    }

    fn get_ready_frame(&mut self) -> Result<Option<DecodedFrame>, error::FFmpegError> {
        let video_ready_ts = self.frame_video_ready.unwrap_or(i64::MAX);
        let audio_ready_ts = self.frame_audio_ready.unwrap_or(i64::MAX);
        let subtitle_ready_ts = self.frame_subtitle_ready.unwrap_or(i64::MAX);

        if video_ready_ts <= audio_ready_ts
            && video_ready_ts <= subtitle_ready_ts
            && video_ready_ts != i64::MAX
        {
            let blank_frame = MediaFrame::new()?;
            self.frame_video_ready = None;
            let ready_frame = mem::replace(&mut self.frame_video, blank_frame);
            Ok(Some(DecodedFrame::Video(VideoFrame { inner: ready_frame })))
        } else if audio_ready_ts <= video_ready_ts
            && audio_ready_ts <= subtitle_ready_ts
            && audio_ready_ts != i64::MAX
        {
            let blank_frame = MediaFrame::new()?;
            self.frame_audio_ready = None;
            let ready_frame = mem::replace(&mut self.frame_audio, blank_frame);
            Ok(Some(DecodedFrame::Audio(AudioFrame { inner: ready_frame })))
        } else if subtitle_ready_ts <= video_ready_ts
            && subtitle_ready_ts <= audio_ready_ts
            && subtitle_ready_ts != i64::MAX
        {
            let blank_frame = MediaFrame::new()?;
            self.frame_subtitle_ready = None;
            let ready_frame = mem::replace(&mut self.frame_subtitle, blank_frame);
            Ok(Some(DecodedFrame::Subtitle(SubtitleFrame {
                inner: ready_frame,
            })))
        } else {
            Ok(None)
        }
    }

    fn read_next_packet(&mut self) -> Result<(), error::FFmpegError> {
        let start = std::time::Instant::now();

        self.packet.reset();
        self.source.read_packet(&mut self.packet)?;

        #[cfg(feature = "trace-hotpath")]
        tracing::trace!("read next packet");

        self.statistics.packet_read_time += start.elapsed();
        self.statistics.packet_read_total += 1;

        Ok(())
    }

    fn dispatch_packet(&mut self) -> Result<(), error::FFmpegError> {
        if self.end_of_packet_stream {
            return Ok(());
        }

        if let Some(video_decoder) = self.decoder_video.as_mut() {
            if self.packet.stream_index as usize == video_decoder.stream.index {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("writing packet to video decoder");
                video_decoder.decoder.write_packet(&mut self.packet)?;
                return Ok(());
            }
        }

        if let Some(audio_decoder) = self.decoder_audio.as_mut() {
            if self.packet.stream_index as usize == audio_decoder.stream.index {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("writing packet to audio decoder");
                audio_decoder.decoder.write_packet(&mut self.packet)?;
                return Ok(());
            }
        }

        if let Some(subtitle_decoder) = self.decoder_subtitle.as_mut() {
            if self.packet.stream_index as usize == subtitle_decoder.stream.index {
                #[cfg(feature = "trace-hotpath")]
                tracing::trace!("writing packet to subtitle decoder");
                subtitle_decoder.decoder.write_packet(&mut self.packet)?;
                return Ok(());
            }
        }

        #[cfg(feature = "trace-hotpath")]
        tracing::trace!("packet did not match any target streams");

        Ok(())
    }

    fn flush(&mut self) -> Result<(), error::FFmpegError> {
        tracing::debug!("flushing decoders");

        if let Some(video_decoder) = self.decoder_video.as_mut() {
            video_decoder.decoder.flush()?;
        }

        if let Some(audio_decoder) = self.decoder_audio.as_mut() {
            audio_decoder.decoder.flush()?;
        }

        if let Some(subtitle_decoder) = self.decoder_subtitle.as_mut() {
            subtitle_decoder.decoder.flush()?;
        }

        tracing::debug!("flushed decoders");

        Ok(())
    }
}

/// The core components that can be accessed for video, audio and subtitles.
pub trait Frame {
    /// Returns the presentation timestamp of the frame.
    fn pts(&self) -> Duration;

    /// Signals if the frame data is backed by hardware (i.e. GPU)
    fn is_hw_backed(&self) -> bool;
}

#[derive(Debug)]
/// A frame which has been decoded from the [InputSource].
pub enum DecodedFrame {
    Video(VideoFrame),
    Audio(AudioFrame),
    Subtitle(SubtitleFrame),
}

impl Frame for DecodedFrame {
    fn pts(&self) -> Duration {
        match self {
            DecodedFrame::Video(frame) => frame.pts(),
            DecodedFrame::Audio(frame) => frame.pts(),
            DecodedFrame::Subtitle(frame) => frame.pts(),
        }
    }

    fn is_hw_backed(&self) -> bool {
        match self {
            DecodedFrame::Video(frame) => frame.is_hw_backed(),
            DecodedFrame::Audio(frame) => frame.is_hw_backed(),
            DecodedFrame::Subtitle(frame) => frame.is_hw_backed(),
        }
    }
}

/// A decoded video frame.
///
/// In the pixel format of one of the target [OutputPixelFormat] formats
/// you configure on the player.
pub struct VideoFrame {
    inner: MediaFrame,
}

impl std::fmt::Debug for VideoFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "VideoFrame(pix_fmt={:?}, resolution={}x{}, planes={}, pts={:?})",
            self.pixel_format(),
            self.width(),
            self.height(),
            self.num_planes(),
            self.pts(),
        )
    }
}

impl VideoFrame {
    #[inline]
    /// Returns the pixel format of the video frame.
    pub fn pixel_format(&self) -> OutputPixelFormat {
        OutputPixelFormat::try_from_av_pix_fmt(self.inner.format)
            .expect("unexpected video pixel format encountered")
    }

    #[inline]
    /// The width of the frame in pixels.
    pub fn width(&self) -> usize {
        self.inner.width as usize
    }

    #[inline]
    /// The height of the frame in pixels.
    pub fn height(&self) -> usize {
        self.inner.height as usize
    }

    #[inline]
    /// Returns the stride of the given plane.
    pub fn stride(&self, index: usize) -> usize {
        assert!(index < self.num_planes(), "index out of range");
        // TODO: Negative values situation possible within this context?
        self.inner.linesize[index] as usize
    }

    #[inline]
    /// The video plane width in pixels.
    pub fn plane_width(&self, index: usize) -> usize {
        assert!(index < self.num_planes(), "index out of range");

        // Logic taken from image_get_linesize().
        if index != 1 && index != 2 {
            return self.width();
        }

        if let Some(desc) = self.pixel_format().descriptor() {
            let s = desc.log2_chroma_w;
            (self.width() + (1 << s) - 1) >> s
        } else {
            self.width()
        }
    }

    #[inline]
    /// The video plane height in pixels.
    pub fn plane_height(&self, index: usize) -> usize {
        assert!(index < self.num_planes(), "index out of range");

        // Logic taken from av_image_fill_pointers().
        if index != 1 && index != 2 {
            return self.height();
        }

        if let Some(desc) = self.pixel_format().descriptor() {
            let s = desc.log2_chroma_w;
            (self.height() + (1 << s) - 1) >> s
        } else {
            self.height()
        }
    }

    #[inline]
    /// Returns the number of video planes within the frame.
    pub fn num_planes(&self) -> usize {
        let mut total = 0;
        for i in 0..8 {
            if self.inner.linesize[i] == 0 {
                break;
            }
            total += 1;
        }
        total
    }

    /// Retrieve the raw data of a given plane.
    ///
    /// If the frame is hardware backed, it will transfer the data
    /// from the device to system memory which may increase latency.
    pub fn plane_data(&mut self, index: usize) -> crate::Result<&[u8]> {
        assert!(index < self.num_planes(), "index out of range");

        if self.is_hw_backed() {
            self.inner.copy_hw_to_software()?;
        }

        let ptr = self.inner.data[index];
        debug_assert!(!ptr.is_null());

        let buffer = unsafe {
            std::slice::from_raw_parts(
                ptr,
                self.stride(index) * self.plane_height(index),
            )
        };

        Ok(buffer)
    }
}

impl Frame for VideoFrame {
    #[inline]
    fn pts(&self) -> Duration {
        pts_to_duration(self.inner.pts, self.inner.time_base)
    }

    #[inline]
    fn is_hw_backed(&self) -> bool {
        !self.inner.hw_frames_ctx.is_null()
    }
}

/// A decoded audio frame.
pub struct AudioFrame {
    inner: MediaFrame,
}

impl std::fmt::Debug for AudioFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AudioFrame(sample_fmt={:?}, samples={}, channels={}, planes={}, pts={:?})",
            self.sample_format(),
            self.num_samples(),
            self.num_channels(),
            self.num_planes(),
            self.pts(),
        )
    }
}

impl AudioFrame {
    #[inline]
    /// Returns the number of audio channels.
    pub fn num_channels(&self) -> usize {
        self.inner.ch_layout.nb_channels as usize
    }

    #[inline]
    /// Returns the number of audio samples per channel.
    pub fn num_samples(&self) -> usize {
        self.inner.nb_samples as usize
    }

    #[inline]
    /// Returns if the audio samples are planar.
    pub fn is_planar(&self) -> bool {
        self.sample_format().is_planar()
    }

    #[inline]
    /// Returns if the audio samples are interleaved/packed.
    pub fn is_packed(&self) -> bool {
        self.sample_format().is_packed()
    }

    #[inline]
    /// Returns the output format of the audio.
    pub fn sample_format(&self) -> SampleFormat {
        SampleFormat::try_from_av_sample_fmt(self.inner.format)
            .expect("unexpected audio sample format encountered")
    }

    #[inline]
    /// Returns the number of audio planes within the frame.
    pub fn num_planes(&self) -> usize {
        if self.inner.linesize[0] == 0 {
            return 0;
        }

        if self.is_packed() {
            1
        } else {
            self.num_channels()
        }
    }

    #[inline]
    /// Retrieve the raw data of a given plane.
    ///
    /// If the frame is hardware backed, it will transfer the data
    /// from the device to system memory which may increase latency.
    pub fn plane_data(&mut self, index: usize) -> crate::Result<&[u8]> {
        assert!(index < self.num_planes(), "index out of range");

        if self.is_hw_backed() {
            self.inner.copy_hw_to_software()?;
        }

        let ptr = self.inner.data[index];
        debug_assert!(!ptr.is_null());

        let buffer = unsafe {
            std::slice::from_raw_parts(ptr, self.inner.linesize[index] as usize)
        };

        Ok(buffer)
    }
}

impl Frame for AudioFrame {
    fn pts(&self) -> Duration {
        pts_to_duration(self.inner.pts, self.inner.time_base)
    }

    fn is_hw_backed(&self) -> bool {
        !self.inner.hw_frames_ctx.is_null()
    }
}

/// A decoded subtitle frame.
///
/// This can be in either text/ASS format or bitmap format.
pub struct SubtitleFrame {
    inner: MediaFrame,
}

impl std::fmt::Debug for SubtitleFrame {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "SubtitleFrame(format={:?})", self.format(),)
    }
}

impl SubtitleFrame {
    fn subtitle_frame(&self) -> &ffmpeg::AVSubtitle {
        unsafe {
            // Based on the source of subtitle_wrap_frame for the new way of handling
            // subtitles from a AVFrame.
            let subtitle_ptr: *const ffmpeg::AVSubtitle =
                (*self.inner.buf[0]).data.cast();
            subtitle_ptr
                .as_ref()
                .expect("subtitle frame should have non-null buf ptr")
        }
    }

    pub fn format(&self) -> SubtitleFormat {
        let subtitle = self.subtitle_frame();
        SubtitleFormat::try_from_subtitle_format(
            subtitle.format as ffmpeg::AVSubtitleType,
        )
        .expect("unsupported subtitle format provided")
    }

    /// Returns the subtitle text content if it is
    /// in a text format.
    pub fn text(&self) -> Option<String> {
        let subtitle = self.subtitle_frame();
        todo!()
    }
}

impl Frame for SubtitleFrame {
    fn pts(&self) -> Duration {
        pts_to_duration(self.inner.pts, self.inner.time_base)
    }

    fn is_hw_backed(&self) -> bool {
        !self.inner.hw_frames_ctx.is_null()
    }
}

#[derive(Debug, Copy, Clone, Default)]
/// Statistics collected from the player around timings, etc...
pub struct PlayerStatistics {
    /// The number of video frames read from the stream and decoded so far.
    pub num_video_frames_decoded: u64,
    /// The number of audio frames read from the stream and decoded so far.
    pub num_audio_frames_decoded: u64,
    /// The number of subtitle frames read from the stream and decoded so far.
    pub num_subtitle_frames_decoded: u64,
    /// The total number of packets read.
    pub packet_read_total: u64,
    /// The total amount of time spent reading packets.
    pub packet_read_time: Duration,
    /// The total number of frames read from all streams.
    pub frames_decoded_total: u64,
    /// The total amount of time spent decoding frames.
    pub frames_decoded_time: Duration,
    /// The total amount of time spent getting frames from the media.
    ///
    /// This includes the time to read packets and decode.
    pub frames_total_time: Duration,
}

struct MediaFrame {
    ptr: *mut ffmpeg::AVFrame,
}

impl MediaFrame {
    fn new() -> Result<Self, error::FFmpegError> {
        let packet = unsafe { ffmpeg::av_frame_alloc() };
        if packet.is_null() {
            Err(error::FFmpegError::custom("failed to allocate frame"))
        } else {
            Ok(Self { ptr: packet })
        }
    }

    fn copy_hw_to_software(&mut self) -> Result<(), error::FFmpegError> {
        if !self.hw_frames_ctx.is_null() {
            #[allow(unused_mut)]
            let mut sw_frame = MediaFrame::new()?;
            let result =
                unsafe { ffmpeg::av_hwframe_transfer_data(sw_frame.ptr, self.ptr, 0) };
            error::convert_ff_result(result)?;
            *self = sw_frame;
        }
        Ok(())
    }

    fn reset(&mut self) {
        unsafe { ffmpeg::av_frame_unref(self.ptr) }
    }
}

impl std::ops::Deref for MediaFrame {
    type Target = ffmpeg::AVFrame;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl std::ops::DerefMut for MediaFrame {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

impl Drop for MediaFrame {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            self.reset();
            unsafe { ffmpeg::av_frame_free(&raw mut self.ptr) };
        }
    }
}

struct TaggedDecoder<D> {
    stream: StreamInfo,
    decoder: D,
}

#[inline]
fn ignore_out_of_data_error(
    result: Result<(), error::FFmpegError>,
) -> Result<bool, error::FFmpegError> {
    match result {
        Ok(()) => Ok(true),
        Err(err) if err.needs_data() => Ok(false),
        Err(err) => Err(err),
    }
}

struct MediaPacket {
    ptr: *mut ffmpeg::AVPacket,
}

impl MediaPacket {
    fn new() -> Result<Self, error::FFmpegError> {
        let packet = unsafe { ffmpeg::av_packet_alloc() };
        if packet.is_null() {
            Err(error::FFmpegError::custom("failed to allocate packet"))
        } else {
            Ok(Self { ptr: packet })
        }
    }

    fn reset(&mut self) {
        unsafe { ffmpeg::av_packet_unref(self.ptr) }
    }
}

impl std::ops::Deref for MediaPacket {
    type Target = ffmpeg::AVPacket;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.ptr }
    }
}

impl std::ops::DerefMut for MediaPacket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.ptr }
    }
}

impl Drop for MediaPacket {
    fn drop(&mut self) {
        if !self.ptr.is_null() {
            self.reset();
            unsafe { ffmpeg::av_packet_free(&raw mut self.ptr) };
        }
    }
}
