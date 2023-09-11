use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=ass");
    println!("cargo:rerun-if-changed=wrapper.h");

    let bindings = bindgen::Builder::default()
        .header("wrapper.h")
        .parse_callbacks(Box::new(bindgen::CargoCallbacks))
        .layout_tests(false)
        .opaque_type("ass_library")
        .opaque_type(".+[P|p]riv")
        .default_enum_style(bindgen::EnumVariation::ModuleConsts)
        .allowlist_function("ass_.+")
        .allowlist_type("ass_.+")
        .allowlist_var("ass_.+")
        .generate()
        .expect("Unable to generate bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
