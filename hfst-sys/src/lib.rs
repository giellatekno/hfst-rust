#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::raw::c_char;

    /// Length of c string
    fn strlen(s: *const c_char) -> usize {
        let mut len = 0;
        while unsafe { *s.add(len) } != 0 {
            len += 1;
        }
        len
    }

    /// Make a String from a c string
    fn c_charptr_to_string(s: *const c_char) -> String {
        let len = strlen(s);
        unsafe { String::from_raw_parts(s as *mut u8, len, len) }
    }

    /// Remove everything between '@' from `s`.
    /// E.g.
    /// "@_____@HEY@____@THERE" => "HEYTHERE"
    fn remove_ats(s: &str) -> String {
        let at_positions = s
            .char_indices()
            .filter_map(|(pos, ch)| (ch == '@').then_some(pos as i64));

        let it = std::iter::once(-1i64)
            .chain(at_positions)
            .chain(std::iter::once(s.len() as i64));

        let mut out = String::new();
        let mut every_other = false;
        let mut a: usize = 0;

        for el in it {
            if every_other {
                out.push_str(&s[a..el as usize]);
            } else {
                a = (el + 1) as usize;
            }
            every_other = !every_other;
        }

        out
    }

    #[test]
    #[ignore]
    fn open_read_close() -> Result<(), String> {
        let path = "/usr/share/giella/sme/analyser-dict-gt-desc.hfstol\0";
        let path = path.as_ptr() as *const c_char;
        let input_stream = unsafe { hfst_input_stream(path) };
        if input_stream.is_null() {
            return Err(format!("input_stream was NULL"));
        }
        assert!(unsafe { !hfst_input_stream_is_bad(input_stream) });

        let tr = unsafe { hfst_transducer_from_stream(input_stream) };
        assert!(!tr.is_null());

        let mut expected_analyses = std::collections::HashMap::new();
        expected_analyses.insert("viessat+V+IV+Imprt+Du1", false);
        expected_analyses.insert("viessut+V+IV+Imprt+Du1", false);
        expected_analyses.insert("viessut+V+IV+Imprt+Du2", false);
        expected_analyses.insert("viessut+V+IV+Ind+Prs+Sg3", false);
        expected_analyses.insert("viessut+V+IV+PrsPrc", false);
        expected_analyses.insert("viessu+N+Sg+Nom", false);

        let lookup_str = "viessu\0".as_ptr() as *const c_char;
        let lookup = unsafe { hfst_lookup(tr, lookup_str) };
        let iter = unsafe { hfst_lookup_iterator(lookup) };

        unsafe {
            let mut w = 0.0f32;
            let mut s: *mut c_char = std::ptr::null_mut();
            while !hfst_lookup_iterator_done(iter) {
                hfst_lookup_iterator_value(
                    iter,
                    &raw mut s,
                    &mut w,
                );

                let rust_string = c_charptr_to_string(s);
                let seen_analysis = remove_ats(&rust_string);
                let Some(v) = expected_analyses.get_mut(seen_analysis.as_str()) else {
                    panic!("got an analysis we did not expect: {}", seen_analysis);
                };
                *v = true;

                hfst_lookup_iterator_next(iter);
            }
        }
        
        let all_seen = expected_analyses.into_values().all(|seen| seen == true);
        assert!(all_seen);

        unsafe { hfst_input_stream_close(input_stream) };
        Ok(())
    }
}
