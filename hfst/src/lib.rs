#![cfg_attr(doc, feature(doc_cfg))]

//! Rust bindings to _some of_ [libhfst](https://hfst.github.io/) (*)
//!
//! This library is ergonomic wrappers around [hfst_sys](https://docs.rs/hfst-sys).

#[cfg(feature = "tokio-actors")]
pub mod transducer_actor;

use hfst_sys;
use std::ffi::{CString, c_float};
use std::os::raw::{c_char, c_void};
use std::path::Path;
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
        s.as_bytes()
            .iter()
            .copied()
            .map(|b| b as c_char)
            .chain(std::iter::once(0 as c_char)),
    )
    .into_boxed_slice();
    assert_eq!(s.len(), strlen(v.as_ptr()));
    v
}

fn c_charptr_to_string(s: *const c_char) -> String {
    let len = strlen(s);
    unsafe { String::from_raw_parts(s as *mut u8, len, len) }
}

/// A stream for reading binary HFST transducers. Often from a file.
/// This structure is a wrapper around the C++ HfstInputStream.
pub struct HfstInputStream {
    // An opaque pointer to an instance of the C++ HfstInputStream class
    inner: *mut c_void,
}

/// A transducer. Wraps the C++ HfstTransducer.
pub struct HfstTransducer {
    // Opaque pointer to a C++ HfstTransducer
    inner: *mut c_void,
}

/// SAFETY: The transducer can move between threads. Nothing will go wrong
/// if one thread creates an HfstTransducer, and then another thread uses it.
/// (This is required in order to be able to store in a Axum's State)
///
/// But, NOTE: HfstTransducer is *NOT* thread-safe: Two diffferent threads that holds
/// a reference to it, can *not* call .e.g `lookup()` at the same time (it SIGSEGVs).
unsafe impl Send for HfstTransducer {}
// WOULD NOT WORK:
//unsafe impl Sync for HfstTransducer {}

/// Errors related to HfstInputStreams.
#[derive(Debug, thiserror::Error)]
pub enum HfstInputStreamError {
    /// File not found, cannot be opened, or libhfst doesn't think this file
    /// is an hfst file. This variant corresponds to the
    /// NotTransducerStreamException in the C++ API.
    #[error("Not a transducer stream")]
    NotTransducerStream,
    /// Stream is at End Of File. This variant corresponds to
    /// `HfstInputStream::is_eof()` in the C++ API.
    #[error("input stream at EOF")]
    Eof,
    /// Bad stream. Essentially a stream is Bad if any OS-level IO errors has
    /// occurred. This variant corresponds to `HfstInputStream::is_bad()` in
    /// the C++ API.
    #[error("Bad input stream")]
    Bad,
    /// The stream is recognized as a type of FST, but the version of libhfst
    /// that is in use, has not been compiled with support for this type of
    /// fst. This variant corresponds to
    /// `ImplementationTypeNotAvailableException` in the C++ API.
    #[error("Implementation type not available")]
    ImplementationTypeNotAvailable,
}

impl HfstInputStream {
    /// Load a file as an HfstInputStream.
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, HfstInputStreamError> {
        use HfstInputStreamError as Error;
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
                // TODO: Use better error handling. Probably this will be sending a
                // pointer to an int to hfst_input_stream(), where it can write the
                // error code.
                Err(Error::NotTransducerStream)
                //match err as u32 {
                //    hfst_sys::NOT_TRANSDUCER_STREAM => {
                //        Err(Error::NotTransducerStream)
                //    }
                //    hfst_sys::END_OF_STREAM => { Err(Error::Eof) }
                //    hfst_sys::IMPLEMENTATION_TYPE_NOT_AVAILABLE => {
                //        Err(Error::ImplementationTypeNotAvailable)
                //    }
                //    _ => unreachable!("all possible err values covered")
                //}
            }
        }
    }

    /// Read the transducers from this HfstInputStream.
    pub fn read_transducers(&self) -> impl Iterator<Item = HfstTransducer> {
        std::iter::from_fn(|| {
            if unsafe { hfst_sys::hfst_input_stream_is_bad(self.inner) } {
                return None;
            } else if unsafe { hfst_sys::hfst_input_stream_is_eof(self.inner) } {
                return None;
            }
            let tr = unsafe { hfst_sys::hfst_transducer_from_stream(self.inner) };
            if tr.is_null() {
                return None;
            }
            return Some(HfstTransducer { inner: tr });
        })
        //let mut transducers = vec![];
        //loop {
        //    let bad = unsafe { hfst_sys::hfst_input_stream_is_bad(handle) };
        //    if bad {
        //        break;
        //    }
        //    let eof = unsafe { hfst_sys::hfst_input_stream_is_eof(handle) };
        //    if eof {
        //        break;
        //    }
        //    let tr = unsafe { hfst_sys::hfst_transducer_from_stream(self.inner) };
        //    if tr.is_null() {
        //        continue;
        //    }
        //    transducers.push(HfstTransducer { inner: tr });
        //}
        //transducers
    }

    /// Return the *one* transducer that exists in this `HfstInputStream` as
    /// [`Some(transducer)`], or return [`None`] if there are no transducers, or
    /// more than one.
    pub fn read_only_transducer(&self) -> Option<HfstTransducer> {
        let mut it = self.read_transducers();
        let transducer = it.next();
        if let Some(_) = it.next() {
            return None;
        }
        transducer
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

/// Represents a handle to a lookup in progress. This structure is returned
/// from [`HfstTransducer::lookup`]. This type implements [`IntoIterator`],
/// to iterate over the results in the lookup.
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
    /// The type of the elements being iterated over. In the lookup case,
    /// the full string, as well as a weight.
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
        seen.insert(
            "sko+N+Msc+Pl+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@",
            false,
        );
        seen.insert(
            "sko+N+Msc+Pl+Nynorsk+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@",
            false,
        );
        seen.insert(
            "sko+N+Msc+Sg+Indef@D.CmpOnly.FALSE@@D.CmpPref.TRUE@@D.NeedNoun.ON@",
            false,
        );
        seen.insert("sko+V+Imp", false);
        seen.insert("sko+V+Inf", false);

        for (result, _weight) in results {
            *seen.get_mut(result.as_str()).unwrap() = true;
        }

        assert!(seen.into_iter().all(|(_k, v)| v));
    }

    // NOTE: This was a test meant to test that HfstTransducer::lookup worked correctly
    // when called from multiple threads. It does not. It segfaults (SIGSEGV). This is
    // expected, as the underlying C++ HfstTransducer is not thread-safe.
    //
    //#[test]
    //fn can_lookup_from_multiple_threads() {
    //    use std::sync::Arc;

    //    let input_stream = HfstInputStream::new(PATH).unwrap();
    //    let mut transducers = input_stream.read_transducers();
    //    let transducer = transducers.pop().expect("exactly 1 transducer");
    //    assert!(transducers.is_empty(), "only 1 transducer");

    //    let transducer = Arc::new(transducer);

    //    let mut threads = vec![];
    //    for _ in 0..1 {
    //        // Error:
    //        // rustc: `*mut c_void` cannot be shared between threads safely
    //        // within `HfstTransducer`, the trait `Sync` is not implemented for `*mut c_void`
    //        // required for `Arc<HfstTransducer>` to implement `Send`
    //        //
    //        // And we *can not* implement Sync for HfstTransducer, see safety comment
    //        // on the impl Send for HfstTransducer line.
    //        let jh = std::thread::spawn({
    //            let transducer = Arc::clone(&transducer);
    //            move || {
    //                let lookup = transducer.lookup("viessu");
    //                for (s, _w) in lookup {
    //                    println!("{s}");
    //                }
    //            }
    //        });
    //        threads.push(jh);
    //    }
    //
    //    for thread in threads {
    //        thread.join().expect("thread didn't panic");
    //    }
    //}
}
