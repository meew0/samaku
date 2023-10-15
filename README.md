# samaku

![Screenshot](https://github.com/meew0/samaku/blob/master/screenshot.png?raw=true)

samaku is an editor for non-destructive visual typesetting of ASS subtitles. It aims to solve the primary problem of
Lua macro-based typesetting workflows, where once a sign has been typeset, it is very hard to make even minor changes
without redoing the sign entirely. Instead of destructive Lua macros, samaku includes a Blender-like node editor for 
non-destructive editing, where a node graph corresponds to a chain of transformations that are instantly, automatically
rerun when the inputs are changed.

Note that samaku is **extremely incomplete** and **absolutely not ready for production use**. Notably, it does not yet
provide any way to save/export edited subtitles. The node editor works in principle, but so far there are only a few,
primitive nodes, with no way to do most of the important tasks in typesetting. Also, there will be many bugs and
other inconveniences.

I make absolutely no promises that I will continue to develop samaku into a usable tool; in fact, it is pretty likely I
will eventually lose interest or become too busy with something else, given how ambitious this project is. Maybe it will
still be useful educationally.

## Project structure

Technology-wise, samaku uses

- Rust,
- [iced](https://github.com/iced-rs/iced) for the UI,
- [libass](https://github.com/libass/libass) for subtitle rendering,
- [Vapoursynth](https://www.vapoursynth.com/) with [LSMASHSource](https://github.com/HomeOfAviSynthPlusEvolution/L-SMASH-Works/blob/master/VapourSynth/README) for reasonably performant precise video decoding,
- [BestSource](https://github.com/vapoursynth/bestsource) for audio decoding,
- and [libmv](https://projects.blender.org/blender/libmv) for motion tracking.

To understand the project structure, you should have a basic idea of how 
[iced projects are structured in general](https://github.com/iced-rs/iced#overview). The application lives in
`src/lib.rs`, with the `Samaku` struct containing the state; the messages are defined in `src/message.rs`. For the most
part, the application consists of a **pane grid**, containing **panes** with their own view and update logic; these live
in the `pane` module. Apart from that, there are the following modules in `src`:

- `media/bindings`: Thin but safe bindings around the various media libraries.
- `media`: More integrated bindings to provide specifically the features samaku needs.
- `model`: Various specific state structs to avoid too much clutter in `lib.rs`.
- `nde/node`: The different nodes that can be used in the node editor.
- `nde/tags`: Code for parsing, manipulating, and emitting ASS override tags. (I intend to eventually extract this
module into its own crate, for use by others.)
- `nde`: Other glue code for non-destructive editing.
- `resources`: Static resources needed for the UI.
- `subtitle`: samaku's internal representations of ASS subtitle data.
- `view`: UI utilities and custom widgets.
- `workers`: Any code that needs to run in its own thread.

Apart from samaku's own code, there are the following modules in the root folder:

- `benches`: [Criterion](https://github.com/bheisler/criterion.rs) benchmarks for ASS tag handling code.
- `tests`: Integration tests, mainly also for ASS tag handling code.
- `test_files`: Static files used for tests.
- `bestsource-sys`: Unsafe FFI bindings for BestSource and
[libp2p](https://github.com/sekrit-twc/libp2p).
- `libass-sys`: Unsafe FFI bindings for libass.

## How to run

Currently only tested on Linux.

- Install dependencies:
    - [libass](https://github.com/libass/libass) (including headers)
    - Vapoursynth (including headers)
    - LSMASHSource for Vapoursynth
    - Dependencies for [libmv-capi-sys](https://github.com/meew0/libmv-capi-sys#dependencies-dynamic-vs-static-linking): 
  [GOMP](https://gcc.gnu.org/projects/gomp/), [SuiteSparse](https://people.engr.tamu.edu/davis/suitesparse.html), and
  [OpenBLAS](https://www.openblas.net/)
- Run `cargo test` to ensure the dependencies have been installed correctly.
- Then, start the program using `cargo run`.

For actually using samaku, please also take a look at `src/keyboard.rs`, which defines global keyboard shortcuts for
functionality that is not yet mapped to any buttons or the like in the UI.

## Licence notes

samaku as a whole is licenced under the GPLv3, whose text is available in the `LICENSE-GPL` file in the project root.
It also includes some code from [arch1t3cht's Aegisub fork](https://github.com/arch1t3cht/Aegisub/), specifically the
`vapoursynth/aegisub_vs.py` script, as well as the scripts in the `src/media/default_scripts` folder, which are all
licensed under the [BSD 3-clause licence](https://github.com/arch1t3cht/Aegisub/blob/feature/LICENCE).

I eventually plan to extract parts of the code and make it available under more permissive licences.
