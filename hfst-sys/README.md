# hfst-sys

Library with raw bindings to `hfst_c`, the c api version of hfst.

## Running the test

There is one test, and it is marked as `#[ignore]`, to not be run automatically
or by tools. The rest requires the north sámi analyser, located in the
apertium nightly place, namely
`/usr/share/giella/sme/analyser-dict-gt-desc.hfstol`.

If you have this file, run `cargo test -- --ignored` to run the test.
