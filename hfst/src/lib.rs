//! Rust bindings to [libhfst](https://hfst.github.io/) (*)
//!
//! This library is ergonomic wrappers around `hfst_sys`.
//!
//! (*) Some (_most_) functionality missing.

use hfst_sys;
use std::ffi::{c_float, CString};
use std::path::Path;
use std::os::raw::{c_char, c_void};
use std::ptr::addr_of_mut;

fn strlen(s: *const c_char) -> usize {
    let mut len = 0;
    while unsafe { *s.add(len) } != 0 {
        len += 1;
    }
    len
}

// CStr and CString exists, but I found them cumbersome
// to work with, and couldn't always get things right using them..
/// Make a boxed c_char slice from a str by copying the bytes,
/// and appending a null byte at the end.
fn str_to_boxed_c_charptr(s: &str) -> Box<[c_char]> {
    let v = Vec::from_iter(
        s
            .as_bytes()
            .iter()
            .copied()
            .map(|b| b as c_char)
            .chain(std::iter::once(0 as c_char))
    ).into_boxed_slice();
    assert_eq!(s.len(), strlen(v.as_ptr()));
    v
}

fn c_charptr_to_string(s: *const c_char) -> String {
    let len = strlen(s);
    unsafe { String::from_raw_parts(s as *mut u8, len, len) }
}

/// Structure used to load fst files.
pub struct HfstInputStream {
    // An opaque pointer to an instance of the C++ HfstInputStream class
    inner: *mut c_void,
}

/// A transducer
pub struct HfstTransducer {
    // Opaque pointer to a C++ HfstTransducer
    inner: *mut c_void,
}

// SAFETY: The transducer can move between threads. Nothing will go wrong
// if one thread creates an HfstTransducer, and then another thread uses it.
// (This is required in order to be able to store in a Axum's State)
unsafe impl Send for HfstTransducer {}

// anders: the code currently in hfst doesn't check errors.
// /// Errors related to HfstInputStreams.
// #[derive(Debug)]
// pub enum HfstInputStreamError {
//     /// File not found, cannot be opened, or libhfst doesn't think this file
//     /// is an hfst file. This variant corresponds to the
//     /// NotTransducerStreamException in the C++ API.
//     NotTransducerStream,
//     /// Stream is at End Of File. This variant corresponds to
//     /// `HfstInputStream::is_eof()` in the C++ API.
//     Eof,
//     /// Bad stream. Essentially a stream is Bad if any OS-level IO errors has
//     /// occurred. This variant corresponds to `HfstInputStream::is_bad()` in
//     /// the C++ API.
//     Bad,
//     /// The stream is recognized as a type of FST, but the version of libhfst
//     /// that is in use, has not been compiled with support for this type of
//     /// fst. This variant corresponds to
//     /// `ImplementationTypeNotAvailableException` in the C++ API.
//     ImplementationTypeNotAvailable,
// }
// */

impl HfstInputStream {
    /// Load a file as an HfstInputStream.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, ()> {
        // this is apparently wrong and/or suboptimal, but going from
        // a Path to a C char* is apparently not straight forward
        let path = CString::new(format!("{}", path.as_ref().display())).unwrap();
        let path = path.as_ptr() as *const c_char;

        //let mut err: c_int = 0;
        unsafe {
            let stream = hfst_sys::hfst_input_stream(path);
            if !stream.is_null() {
                Ok(Self { inner: stream })
            } else {
                return Err(());
                // anders: will be used when libhfst gives an error code
                // on stream reading failure
                //use HfstInputStreamError::*;
                //match err as u32 {
                //    hfst_sys::NOT_TRANSDUCER_STREAM => {
                //        Err(NotTransducerStream)
                //    }
                //    hfst_sys::END_OF_STREAM => { Err(Eof) }
                //    hfst_sys::IMPLEMENTATION_TYPE_NOT_AVAILABLE => {
                //        Err(ImplementationTypeNotAvailable)
                //    }
                //    _ => unreachable!("all possible err values covered")
                //}
            }
        }
    }

    /// Read the transducers from this HfstInputStream.
    pub fn read_transducers(&self) -> Vec<HfstTransducer> {
        let handle = self.inner;
        let mut transducers = vec![];
        loop {
            let bad = unsafe { hfst_sys::hfst_input_stream_is_bad(handle) };
            if bad {
                break;
            }
            let eof = unsafe { hfst_sys::hfst_input_stream_is_eof(handle) };
            if eof {
                break;
            }
            let tr = unsafe { hfst_sys::hfst_transducer_from_stream(self.inner) };
            if tr.is_null() {
                continue;
            }
            transducers.push(HfstTransducer { inner: tr });
        }
        transducers
    }
}

impl HfstTransducer {
    /// Look up the string `s` in this `Transducer`.
    pub fn lookup(&self, s: &str) -> HfstLookup {
        let sp = str_to_boxed_c_charptr(s);
        assert_eq!(strlen(sp.as_ptr()), s.len());
        let handle = unsafe { hfst_sys::hfst_lookup(self.inner, sp.as_ptr()) };
        assert!(!handle.is_null());
        HfstLookup { handle }
    }
}

impl Drop for HfstInputStream {
    fn drop(&mut self) {
        unsafe {
            hfst_sys::hfst_input_stream_close(self.inner);
            //hfst_sys::hfst_input_stream_free(self.inner);
        }
    }
}

/// A lookup. You get this type back
pub struct HfstLookup {
    handle: *mut c_void,
}

impl IntoIterator for HfstLookup {
    type Item = (String, f32);
    type IntoIter = HfstLookupIterator;

    fn into_iter(self) -> Self::IntoIter {
        let inner = unsafe { hfst_sys::hfst_lookup_iterator(self.handle) };

        HfstLookupIterator { inner }
    }
}

pub struct HfstLookupIterator {
    // the underlying HfstLooup
    //lookup_handle: HfstLookup,
    // Opaque pointer to a "struct ResultIterator"
    inner: *mut hfst_sys::ResultIterator,
}

impl Iterator for HfstLookupIterator {
    type Item = (String, f32);

    fn next(&mut self) -> Option<Self::Item> {
        if unsafe { hfst_sys::hfst_lookup_iterator_done(self.inner) } {
            None
        } else {
            let mut s: *mut c_char = std::ptr::null_mut();
            let w: c_float = 0.0;
            unsafe {
                hfst_sys::hfst_lookup_iterator_value(
                    self.inner,
                    addr_of_mut!(s),
                    &w as *const _ as *mut _,
                );
            }
            let rust_string = c_charptr_to_string(s);
            unsafe { hfst_sys::hfst_lookup_iterator_next(self.inner) };

            // c_float is always rust f32, right?
            Some((rust_string, w))
        }
    }
}

#[cfg(test)]
mod tests {
    const PATH: &'static str = "/usr/share/giella/nob/analyser-gt-desc.hfstol";
    use super::*;

    #[test]
    fn can_open_inputstream() {
        let input_stream = HfstInputStream::new(PATH);
        assert!(input_stream.is_ok());
    }

    #[test]
    fn errors_on_opening_nonexistant() {
        let input_stream = HfstInputStream::new("/this/path/doesnt/exist");
        assert!(matches!(input_stream, Err(())));
    }

    #[test]
    fn can_lookup() {
        let input_stream = HfstInputStream::new(PATH).unwrap();
        let transducers = input_stream.read_transducers();
        let transducer = transducers
            .first()
            .expect("the hfst input stream has at least one transducer");
        let query = "sko";
        let results = transducer.lookup(query);
        let mut seen = std::collections::HashMap::new();
        seen.insert("sko+N+Msc+Pl+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@", false);
        seen.insert("sko+N+Msc+Pl+Nynorsk+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@", false);
        seen.insert("sko+N+Msc+Sg+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@", false);
        seen.insert("sko+V+Imp", false);
        seen.insert("sko+V+Inf", false);

        for (result, _weight) in results {
            *seen.get_mut(result.as_str()).unwrap() = true;
        }

        assert!(seen.into_iter().all(|(_k, v)| v));
    }
}
