use libav_player::{
    Frame,
    InputSource,
    MediaPlayerBuilder,
    MediaType,
    OutputPixelFormat,
    PlayerError,
};

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let source = InputSource::open_file("samples/idol-x265-120fps.mp4")?;

    let stream = source.find_best_stream(MediaType::Video, None)?.unwrap();
    dbg!(stream);

    let mut player = MediaPlayerBuilder::for_source(source)
        .with_target_pixel_formats(vec![OutputPixelFormat::Nv12])
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
