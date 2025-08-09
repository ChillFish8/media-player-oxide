# A media player toolkit for Rust

A (relatively) efficient media player system for wgpu based systems & friends, built on top of FFmpeg/libav.

The main goal of this project is to learn something new as I try and built a media player app in Rust
without touching a ton of platform specific APIs.

It tries to provide a relatively simple to use API and leaves integrating the wgpu textures up to the
end user, note we still depend on wgpu for some reason I mention further down.

## Modules

- **[libav-player](/libav-player)** - A low-ish level wrapper around libav providing you with a player API for decoding
  videos and retrieving their video APIs rather than you having to interact with libav yourself.
- **[wgpu-player-toolkit](/wgpu-player-toolkit)** - A higher level player API for taking frames decoded by the `libav-player`
  project and rendering them with wgpu.
- **[wgpu-player-toolkit-demo](/wgpu-player-toolkit-demo)** - An example demo app that uses `libav-player` and 
  `wgpu-player-toolkit` to provide a basic video player, similar to ffplay.

## Development

This library dynamically links the libav* libraries, you can get a copy of them from the
[FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) releases, just make sure to get the `shared` variants.

By default, the system requires the `FFMPEG_INCLUDE_DIR` and `FFMPEG_PKG_CONFIG_PATH` set to compile,
if you are working on the library itself or this repo, you can drop one of the aforementioned releases
into a `./ffmpeg` folder and the `.cargo/config.toml` is pre-configured to specify the required env vars.

You can also enable the `link-system-ffmpeg` or `link-vcpkg-ffmpeg` flag to link to the FFmpeg headers and libs available from the
system or vcpkg respectively.

## How it works

The core of the player is quite* simple, FFmpeg does a lot of the heavy lifting for us, but we still have some
very specific logic going on when we are transferring frames from FFmpeg to our render API.

A high level view of the playback rendering pipeline looks like: TODO!

