# libav-player

A low level media player toolkit designed to aid in building video players using Rust.

As the name implies this crate is powered by FFmpeg's libav libraries and expects FFmpeg 7.1 currently.

## Features

- Built on top of FFmpeg
- Fast and efficient, we target videos of 4k 120fps+ 
  * We effectively match what you would get with an `ffmpeg` command transcoding the video.
  * Currently on my system I can do about 4k 200fps maximum.
  * Your actual milage will vary depending on your hardware and GPU
- Minimal dependencies, you only need to link or distribute the libav libraries.
- Simple state machine API, you dictate the pacing of retrieving frames, we just give you the frames
  themselves, you decide how to render them or process them.
- Subtitles supported with automatic burn in via libass.

## Why this and not gstreamer or similar?

I built this project mostly because I wanted to build an alternative Jellyfin desktop client with wgpu, but
supporting hardware decoding and efficient rendering is hard, Rust also has a fairly immature ecosystem in this space
so it really comes down to writing bindings to libmpv, gstreamer or low level libav usage. 
I originally tried gstreamer, which is still an awesome, well maintained toolkit, but could not get it to handle
4k 120fps fast enough without dropping frames or delays.

## Example

```rust
use libav_player::{
    Frame,
    InputSource,
    MediaPlayerBuilder,
    MediaType,
    OutputPixelFormat,
    PlayerError,
};

fn main() -> anyhow::Result<()> {
    let source = InputSource::open_file("samples/idol-x265-120fps.mp4")?;

    let stream = source.find_best_stream(MediaType::Video, None)?.unwrap();
    println!("got video stream: {stream:?}");

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
    println!("completed read!");

    Ok(())
}
```