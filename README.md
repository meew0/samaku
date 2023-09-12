## How to run

- Install dependencies:
    - libass (including headers)
    - Vapoursynth (including headers)
    - LSMASHSource for Vapoursynth
- Build the packaged BestSource:
    - Install Meson and FFmpeg
    - `cd bestsource-sys`
    - `meson setup build`
    - `ninja -C build`
- `cargo run`
