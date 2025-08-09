# wgpu-player-toolkit 

Render output frames from `libav-player` to WGPU textures.

This library is designed to do most of the heavy lifting getting decoded video frames uploading them into wgpu textures
that you can use. This library also handles audio playback and synchronizing video frames to audio.

*NOTE: This toolkit does not handle creating and pulling frames from `libav-player` directly, instead it gives you a channel
to push ready frames into.*