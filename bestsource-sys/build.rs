extern crate cc;

use std::env;
use std::path::PathBuf;

fn libp2p_compiler() -> cc::Build {
    let mut libp2p_compiler = cc::Build::new();
    libp2p_compiler.cpp(true).include("libp2p");

    // TODO: allow manually configuring whether to use SIMD or not
    if cfg!(target_arch = "x86_64") {
        libp2p_compiler.define("P2P_SIMD", Some(""));
    }

    libp2p_compiler
}

fn main() {
    // Compile BestSource itself, together with our C wrapper
    let bestsourcew_sources = [
        "bestsource/src/audiosource.cpp",
        "bestsource/src/videosource.cpp",
        "bestsource/src/tracklist.cpp",
        "bestsource/src/bsshared.cpp",
        "wrapper/wrapper.cpp",
    ];

    cc::Build::new()
        .cpp(true)
        .include("bestsource/src")
        .include("bestsource/libp2p")
        .files(bestsourcew_sources.iter())
        .flag_if_supported("-Wno-sign-compare")
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-reorder")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("bestsourcew");

    // Compile libp2p as well. BestSource only uses this for video decoding,
    // so it is not strictly necessary to link it here (as we only use BestSource for audio).
    // However, we can use it for our own video decoding purposes
    let libp2p_sources = vec![
        "bestsource/libp2p/p2p_api.cpp",
        "bestsource/libp2p/v210.cpp",
        "bestsource/libp2p/simd/cpuinfo_x86.cpp",
        "bestsource/libp2p/simd/p2p_simd.cpp",
    ];

    libp2p_compiler()
        .files(libp2p_sources.iter())
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("p2p_main");

    // TODO: allow manually configuring whether to use SIMD or not
    if cfg!(target_arch = "x86_64") {
        let libp2p_sse41_source = "bestsource/libp2p/simd/p2p_sse41.cpp";
        libp2p_compiler()
            .flag("-msse4.1")
            .file(libp2p_sse41_source)
            .compile("p2p_sse41");
        println!("cargo:rerun-if-changed={}", libp2p_sse41_source);
    }

    for source in bestsourcew_sources.iter().chain(libp2p_sources.iter()) {
        println!("cargo:rerun-if-changed={}", source);
    }
    println!("cargo:rerun-if-changed=wrapper/wrapper.h");

    // BestSource dependencies
    println!("cargo:rustc-link-lib=avcodec");
    println!("cargo:rustc-link-lib=avformat");
    println!("cargo:rustc-link-lib=avutil");
    println!("cargo:rustc-link-lib=xxhash");

    let bindings = bindgen::Builder::default()
        .header("wrapper/wrapper.h")
        .header("bestsource/libp2p/p2p_api.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
