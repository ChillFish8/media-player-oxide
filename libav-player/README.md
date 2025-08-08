# libav-player

A low level media player toolkit designed to aid in building video players using Rust.

As the name implies this crate is powered by FFmpeg's libav libraries and expects FFmpeg 7.1 currently.

## Features

- Fast and efficient, we target videos of 4k 120fps+ 
  * We effectively match what you would get with an `ffmpeg` command transcoding the video.
  * Currently on my system I can do about 4k 200fps maximum.
  * Your actual milage will vary depending on your hardware and GPU
- Minimal dependencies, you only need to link or distribute the libav libraries.
- Simple state machine API, you dictate the pacing of retrieving frames, we just give you the frames
  themselves, you decide how to render them or process them.
- Subtitles supported

## Why this and not gstreamer or similar?

I built this project mostly because I wanted to build an alternative Jellyfin desktop client with wgpu, but
supporting hardware decoding and efficient rendering is hard, Rust also has a fairly immature ecosystem in this space
so it really comes down to writing bindings to libmpv, gstreamer or low level libav usage. 
I originally tried gstreamer, which is still an awesome, well maintained toolkit, but could not get it to handle
4k 120fps fast enough without dropping frames or delays.
