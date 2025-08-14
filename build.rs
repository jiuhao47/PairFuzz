use std::{env, path::PathBuf};

fn main() -> anyhow::Result<()> {
    if env::var("CARGO_BIN_NAME").map_or(true, |v| v != "libafl_cc") {
        // Add rerun-if-changed for the pre-built static library
        println!("cargo:rerun-if-changed=libdifferential.a");

        // Configure and generate bindings for libpng and libspng harness
        let bindings = bindgen::builder()
            .header("interface/diffray_harness.h")
            .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
            .generate()?;
        // Write the generated bindings to an output file.
        let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
        bindings
            .write_to_file(out_path.join("bindings.rs"))
            .expect("Couldn't write bindings!");

        // Tell Cargo to link with the pre-built differential.a library
        // Search in the project root directory
        println!("cargo:rustc-link-search=native=.");

        // Alternative examples for different locations:
        // println!("cargo:rustc-link-search=native=./lib");  // Search in ./lib directory
        // println!("cargo:rustc-link-search=native=/absolute/path/to/libs");  // Absolute path
        // let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        // println!("cargo:rustc-link-search=native={}/custom_lib_dir", manifest_dir);

        // Link ASan first (must come before other libraries)
        println!("cargo:rustc-link-lib=asan"); // AddressSanitizer runtime

        // Then link our static library
        println!("cargo:rustc-link-lib=static=differential");

        // Finally link system libraries
        println!("cargo:rustc-link-lib=stdc++"); // C++ standard library
        println!("cargo:rustc-link-lib=m"); // math library
        println!("cargo:rustc-link-lib=z"); // zlib
    }
    Ok(())
}
