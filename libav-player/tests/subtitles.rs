use libav_player::{
    DecodedFrame,
    Frame,
    InputSource,
    MediaPlayerBuilder,
    MediaType,
    OutputPixelFormat,
    PlayerError,
};

#[test]
fn test_subtitle_decode() -> anyhow::Result<()> {
    unsafe {
        rusty_ffmpeg::ffi::av_log_set_level(rusty_ffmpeg::ffi::AV_LOG_VERBOSE as i32);
    };
    let _ = tracing_subscriber::fmt::try_init();

    let source = InputSource::open_file("../media/subtitles.mp4")?;

    let stream = source.find_best_stream(MediaType::Subtitle, None)?.unwrap();
    dbg!(&stream);

    let mut player = MediaPlayerBuilder::for_source(source)
        .with_subtitle_stream(Some(stream.index))
        .build()?;

    player.play()?;

    let mut frame_count = 0;
    loop {
        let frame = match player.process_next_frame() {
            Err(PlayerError::EndOfStream) => break,
            Err(other) => return Err(other.into()),
            Ok(frame) => frame,
        };
        frame_count += 1;
        assert!(!frame.is_hw_backed());
    }

    dbg!(player.statistics(), frame_count);
    tracing::info!("completed read");

    Ok(())
}
