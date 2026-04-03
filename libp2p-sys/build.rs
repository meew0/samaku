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
    let libp2p_sources = vec![
        "libp2p/p2p_api.cpp",
        "libp2p/v210.cpp",
        "libp2p/simd/cpuinfo_x86.cpp",
        "libp2p/simd/p2p_simd.cpp",
    ];

    libp2p_compiler()
        .files(libp2p_sources.iter())
        .flag_if_supported("-Wno-missing-field-initializers")
        .flag_if_supported("-Wno-unused-parameter")
        .compile("p2p_main");

    // TODO: allow manually configuring whether to use SIMD or not
    if cfg!(target_arch = "x86_64") {
        let libp2p_sse41_source = "libp2p/simd/p2p_sse41.cpp";
        libp2p_compiler()
            .flag("-msse4.1")
            .file(libp2p_sse41_source)
            .compile("p2p_sse41");
        println!("cargo:rerun-if-changed={}", libp2p_sse41_source);
    }

    for source in libp2p_sources {
        println!("cargo:rerun-if-changed={}", source);
    }
    let bindings = bindgen::Builder::default()
        .header("libp2p/p2p_api.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
