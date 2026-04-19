use std::env;
use std::path::PathBuf;

fn main() {
    println!("cargo:rustc-link-lib=ass");
    println!("cargo:rerun-if-changed=wrapper.h");
    println!("cargo:rerun-if-env-changed=LIBASS_INCLUDE_DIR");
    println!("cargo:rerun-if-env-changed=LIBASS_LIB_DIR");

    let mut builder = bindgen::Builder::default().header("wrapper.h");

    if let Ok(include_dir) = env::var("LIBASS_INCLUDE_DIR") {
        builder = builder.clang_arg(format!("-I{}", include_dir));
    }

    // On MSVC Windows, tell clang the target so va_list is defined as char* (not __va_list_tag*)
    #[cfg(all(target_os = "windows", target_env = "msvc"))]
    {
        builder = builder.clang_arg("--target=x86_64-pc-windows-msvc");
    }

    if let Ok(lib_dir) = env::var("LIBASS_LIB_DIR") {
        println!("cargo:rustc-link-search=native={}", lib_dir);
    }

    let bindings = builder
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
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
