# samaku

[![Discord Server](https://img.shields.io/discord/1510048036250718379?label=&labelColor=6A7EC2&logo=discord&logoColor=ffffff&color=5865F2)](https://discord.gg/dRhz7gA7Hb)

![Screenshot](https://github.com/meew0/samaku/blob/master/screenshot.png?raw=true)

samaku is an editor for non-destructive visual typesetting of ASS (Advanced SubStation Alpha) subtitles. It aims to solve the primary problem of Lua macro-based typesetting workflows, where once a sign has been typeset, it is very hard to make even minor changes without redoing the sign entirely. Instead of destructive Lua macros, samaku includes a Blender-like node editor for non-destructive editing, where a node graph corresponds to a chain of transformations that are instantly, automatically rerun when the inputs are changed.

A secondary goal of samaku is to be an extremely high-performance ASS subtitle editor, effortlessly supporting giant subtitle files (several hundred megabytes or more, hundreds of thousands of events or more).

Note that samaku is currently in a pre-alpha state. It is **very incomplete** and **not yet ready for production use**. Most of the foundational work has been done and the app can be used for basic subtitle editing, and the node editor with its NDE paradigm works in principle. However, so far the selection of nodes is very limited, with no way to do many important tasks in typesetting. Also, there will be many bugs and other inconveniences. The app is still lacking significant polish in most areas.

I make no promises that I will continue to develop samaku into a usable tool. I tend to work on it in short bursts every now and then, with significant latency periods in between. Nevertheless, I am glad if it can be of interest for someone, even educationally.

For an overview of what is theoretically planned at the moment, you can take a look at the [roadmap](https://github.com/users/meew0/projects/1).

## Project structure

Technology-wise, samaku uses

- Rust,
- [iced](https://github.com/iced-rs/iced) for the UI (with [iced_nodegraph](https://github.com/tuco86/iced_nodegraph) for the node editor),
- [libass](https://github.com/libass/libass) for subtitle rendering,
- [ffms2](https://github.com/FFMS/ffms2) + [ffms2-sys](https://crates.io/crates/ffms2-sys) for audio and video decoding,
- and [libmv](https://projects.blender.org/blender/libmv) + [libmv-capi-sys](https://crates.io/crates/libmv-capi-sys)
  for motion tracking.

See the [Cargo.toml](https://github.com/meew0/samaku/blob/master/Cargo.toml) for a full list of dependencies.

To understand the project structure, you should have a basic idea of how [iced projects are structured in general](https://github.com/iced-rs/iced#overview). The application lives in `src/lib.rs`, with the `Samaku` struct containing the state; the messages are defined in `src/message.rs`. The global update method is in `src/update.rs`. For the most part, the application consists of a **pane grid**, containing **panes** with their own view and update logic; these live in the `pane` module. Apart from that, there are the following modules in `src`:

- `media/bindings`: Thin but safe bindings around the various media libraries.
- `media`: More integrated bindings to provide specifically the features samaku needs.
- `model`: Various specific state structs to avoid too much clutter in `lib.rs`.
- `nde/node`: The different nodes that can be used in the node editor.
- `nde/tags`: Code for parsing, manipulating, and emitting ASS override tags. (I intend to eventually extract this module into its own crate, for use by others.)
- `nde`: Other glue code for non-destructive editing.
- `pane`: Panes of the pane grid (as mentioned above).
- `resources`: Static resources needed for the UI.
- `subtitle`: samaku's internal representations of ASS subtitle data.
- `view`: UI utilities and custom widgets.
- `workers`: Any code that needs to run in its own thread.

Apart from samaku's own code, there are the following modules in the root folder:

- `benches`: [Criterion](https://github.com/bheisler/criterion.rs) benchmarks for some performance-critical components.
- `tests`: Integration tests, mainly for ASS tag handling code.
- `test_files`: Static files used for tests.
- `libass-sys`: Unsafe FFI bindings for libass.
- `packaging`: Utilities for packaging samaku for distribution.

## How to run

If you want to try out samaku yourself, prebuilt nightlies for Linux and Windows are available at nightly.link [here](https://nightly.link/meew0/samaku/workflows/ci/master). The Linux build contains an AppImage that should be ready to run out of the box. The Windows build includes required DLLs; the zip file should be extracted to a folder and then `samaku.exe` can be run from there.

Keep in mind that samaku is not yet ready for production. There may be data loss. In particular, the project/filter file formats will change between revisions, so do not rely on being able to open a file created with an earlier revision in a later revision.

## How to use

You can use the menu at the top to load subtitle and media files. Then, use the controls within the panes to manage events, filters, motion tracks, etc.

Please also take a look at `src/keyboard.rs`, which defines global keyboard shortcuts for functionality that is not yet mapped to any buttons or the like in the UI. In particular, `F2` and `F3` create new panes; `F4` deletes a pane; and `Shift+F4` clears a pane such that something new can be assigned.

Detailed usage instructions TBD.

## How to build

So far, building samaku has been tested on Linux and Windows.

### Linux

- Install dependencies:
    - [libass](https://github.com/libass/libass) (including headers)
    - [ffms2](https://github.com/FFMS/ffms2) (including headers)
- Run `cargo test` to ensure the dependencies have been installed correctly.
- Then, start the program using `cargo run` (debug mode) or `cargo run --release` (release mode).

It is also possible to build samaku as an AppImage using the `packaging/build-appimage.sh` script.

### Windows

See [WINDOWS_BUILD.md](https://github.com/meew0/samaku/blob/master/WINDOWS_BUILD.md).

## License notes

samaku as a whole is licensed under the GPLv3, whose text is available in the `LICENSE-GPL` file in the project root.

I plan to eventually extract parts of the code (in particular, the ASS override tag processing code) and make it available under more permissive licenses.
