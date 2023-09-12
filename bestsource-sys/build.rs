extern crate cc;

use std::env;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let bestsource_library_dir = Path::new(&manifest_dir).join("build");
    if !bestsource_library_dir.join("libbestsourcew.so").exists() {
        panic!("Missing compiled libbestsourcew.so! Please build BestSource according to the instructions in the README.");
    }

    println!("cargo:rustc-link-lib=bestsourcew");
    println!(
        "cargo:rustc-link-search=native={}",
        bestsource_library_dir.display()
    );
    println!("cargo:rerun-if-changed=meson.build");
    println!("cargo:rerun-if-changed=wrapper/wrapper.cpp");
    println!("cargo:rerun-if-changed=wrapper/wrapper.h");

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
