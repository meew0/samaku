extern crate cc;

use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=dylib=bestsource");
    println!("cargo:rustc-link-search=native=/usr/lib/vapoursynth"); // TODO cross-platform
    println!("cargo:rerun-if-changed=wrapper.h");

    cc::Build::new()
        .cpp(true)
        .file("wrapper/wrapper.cpp")
        .compile("libbestsource_wrapper.a");

    let bindings = bindgen::Builder::default()
        .header("wrapper/wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .layout_tests(false)
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
