use std::env;
use std::path::Path;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let bestsource_library_dir = Path::new(&manifest_dir).join("bestsource-sys/build");
    println!(
        "cargo:rustc-env=LD_LIBRARY_PATH={}",
        bestsource_library_dir.display()
    );
}
