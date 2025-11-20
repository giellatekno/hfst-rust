# hfst-sys

Library with raw bindings to `hfst_c`, the c api version of hfst.


## Justfile

To generate bindings, run `just bindings` (or `just b`). See all just recipies with
just `just`.


## Pre-generated bindings

The bindings are generated with build.rs, but it quieres the system for hfst, which
the build machinery on crates.io does not have. Therefore, we run the build on our
machines, before pushing to crates.io.

https://rust-lang.github.io/rust-bindgen/command-line-usage.html


## Running the test

There is one test, and it is marked as `#[ignore]`, to not be run automatically
or by tools. The rest requires the north sámi analyser, located in the
apertium nightly place, namely
`/usr/share/giella/sme/analyser-dict-gt-desc.hfstol`.

If you have this file, run `cargo test -- --ignored` to run the test.
