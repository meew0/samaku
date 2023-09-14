# samaku

A subtitle editor, eventually, maybe.

![Screenshot](https://github.com/meew0/samaku/blob/master/screenshot.jpg?raw=true)

For a long time I have had a vision of a subtitle editor that allowed for non-destructive typesetting, where you could
create complex signs using a Blender-style node editor instead of painfully re-running Lua macros over and over. I have
attempted to realise this vision several times now (by modifying Aegisub to have NDE capabilities, or modifying Blender
to have subtitle editing capabilities...), but I never got very far. This project is my current attempt, implemented
from scratch using Rust,  [iced](https://github.com/iced-rs/iced) for the UI, [libass](https://github.com/libass/libass)
for subtitle rendering, [Vapoursynth](https://www.vapoursynth.com/)
with [LSMASHSource](https://github.com/HomeOfAviSynthPlusEvolution/L-SMASH-Works/blob/master/VapourSynth/README) for
reasonably performant precise video decoding, and [BestSource](https://github.com/vapoursynth/bestsource) for audio
decoding.

While that list may sound impressive, so far there isn't very much beyond bindings for all these, and some basic code to
tie them together. In particular, there's none of the NDE features I imagined. Many other features necessary for
actually editing subtitle files, like saving/exporting, are also still missing. So **this project is not yet in any
usable state**.

I make absolutely no promises that I will continue to develop samaku into a usable tool; in fact, it is pretty likely I
will eventually lose interest or become too busy with something else, given how ambitious this project is. Maybe it will
still be useful educationally.

## How to run

Currently only tested on Linux.

- Install dependencies:
    - [libass](https://github.com/libass/libass) (including headers)
    - Vapoursynth (including headers)
    - LSMASHSource for Vapoursynth
- Build the packaged BestSource:
    - Install Meson and FFmpeg
    - `cd bestsource-sys`
    - `meson setup build`
    - `ninja -C build`
- `cargo run --release` (smooth video playback requires compiling in release mode, even on powerful systems, until I get
  around to optimising it)
