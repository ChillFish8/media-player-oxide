# A media player for wgpu

**_[Please see the maintenance section before using this libray.](#maintenance)_**

A (relatively) efficient media play for wgpu based systems, built on top of FFmpeg/libav.

The main goal of this project is to learn something new as I try and built a media player app in Rust
without touching a ton of platform specific APIs.

It tries to provide a relatively simple to use API and leaves integrating the wgpu textures up to the
end user, note we still depend on wgpu for some reason I mention further down.

## Features

- Most of the FFmpeg toolset as far as supported files and input formats.
  * Your FFmpeg libraries must be compiled with support for the formats you want to read and sources you want to
    ingest from.
- Hardware accelerator support like VAAPI, CUDA, VULKAN, etc... The affinity the player has for
  each accelerator can be configured.
- Decoding to `nv12`, `p010le` (`yuv420p10le`) or `rgba`.
  * Please be aware that you need hardware capable of decoding videos fast enough, both `rgba` and `yuv420p10le`
    are much heavier than `nv12` and if your GPU doesn't support `nv12` then it probably cannot handle `rgba` or
    `yuv420p10le` conversion.
  * `yuv420p10le` is normally the HDR10 / Dolby Vision formats but be aware wgpu currently does not support HDR
    as far as I am aware, so you would need to bring your own renderer.

## Efficiency

Although this system is built on top of FFmpeg, a very mature and well-made set of libraries, this player
is not as efficient as something like VLC or MPV when you have hardware decoding enabled, this is 
because [currently](https://github.com/gfx-rs/wgpu/issues/3145) we cannot take frames rendered 
by the hardware decoders (if available) and upload them to wgpu going a GPU <-> GPU copy or even no
copies of the GPU memory at all. So for now we must do a GPU <-> CPU <-> GPU copy which causes
a certain amount of additional CPU overhead. That being said, this player was built with both systems
in mind and should not bottleneck on a single CPU core doing all the copies. 
I do plan on supporting the fast paths when available.


## Maintenance

This project was mostly a test and a learning resource...

Please be aware that this project is not really something I am planning to continue to maintain unless 
I need to use it or update it myself, I have a lot of other commitments going on already and don't really
have the time to keep this library constantly up to date or add new features.

If someone dedicated or the community wants to take over the project and continue to improve it you have
my blessing :) 

## Development

This library dynamically links the libav* libraries, you can get a copy of them from the
[FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) releases, just make sure to get the `shared` variants.

By default, the system requires the `FFmpeg_INCLUDE_DIR` and `FFmpeg_PKG_CONFIG_PATH` set to compile,
if you are working on the library itself or this repo, you can drop one of the aforementioned releases
into a `./FFmpeg` folder and the `.cargo/config.toml` is pre-configured to specify the required env vars.

You can also enable the `link-system-FFmpeg` or `link-vcpkg-FFmpeg` flag to link to the FFmpeg headers and libs available from the
system or vcpkg respectively.

## How it works

The core of the player is quite* simple, FFmpeg does a lot of the heavy lifting for us, but we still have some
very specific logic going on when we are transferring frames from FFmpeg to our render API.

A high level view of the playback rendering pipeline looks like: TODO!

