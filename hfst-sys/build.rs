use std::env;
use std::path::PathBuf;

fn main() -> Result<(), ()> {
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());

    if std::env::var("DOCS_RS").is_ok() {
        std::fs::copy("src/expanded.rs", out_path.join("bindings.rs"))
            .expect("Couldn't copy pre-expanded bindings on docs.rs");
        return Ok(());
    }
    println!("cargo:rerun-if-changed=build.rs");

    let hfst_lib = pkg_config::Config::new()
        .atleast_version("0.0.0")
        .probe("hfst_c")
        .map_err(|e| panic!("{:?}", e))?;

    for include_path in hfst_lib.include_paths {
        println!("cargo:rerun-if-changed={}", include_path.display());
    }

    for lib_dir in hfst_lib.link_paths {
        println!("cargo:rustc-link-search={}", lib_dir.display());
    }

    for lib in hfst_lib.libs {
        println!("cargo:rustc-link-lib={lib}");
    }

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let bindings = bindgen::Builder::default()
        // The input header we would like to generate bindings for.
        // .hpp wrapper, so it understands "extern C"", etc
        //.header("/usr/include/hfst/hfst.h")
        .header("wrapper.hpp")
        //.allowlist_item("NOT_TRANSDUCER_STREAM")
        //.allowlist_item("END_OF_STREAM")
        //.allowlist_item("IMPLEMENTATION_TYPE_NOT_AVAILABLE")
        //.allowlist_item("OTHER")
        .allowlist_item("hfst_empty_transducer")
        .allowlist_item("hfst_input_stream")
        .allowlist_item("hfst_input_stream_close")
        .allowlist_item("hfst_input_stream_free")
        .allowlist_item("hfst_input_stream_is_eof")
        .allowlist_item("hfst_input_stream_is_bad")
        .allowlist_item("hfst_transducer_from_stream")
        //.allowlist_item("hfst_lookup_begin")
        //.allowlist_item("hfst_lookup_results")
        .allowlist_item("hfst_lookup")
        .allowlist_item("hfst_lookup_iterator")
        .allowlist_item("hfst_lookup_iterator_value")
        .allowlist_item("hfst_lookup_iterator_next")
        .allowlist_item("hfst_lookup_iterator_free")
        .allowlist_item("hfst_lookup_iterator_done")
        //.allowlist_function("hfst_input_stream_from_file")
        //.allowlist_function("hfst_input_stream_free")

        // Tell cargo to invalidate the built crate whenever any of the
        // included header files changed.
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        // Finish the builder and generate the bindings.
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    Ok(())
}
