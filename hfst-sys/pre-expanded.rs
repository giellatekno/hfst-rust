#[repr(C)]
pub struct ResultIterator {
    pub begin: *mut ::std::os::raw::c_void,
    pub end: *mut ::std::os::raw::c_void,
}
#[automatically_derived]
impl ::core::fmt::Debug for ResultIterator {
    #[inline]
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        ::core::fmt::Formatter::debug_struct_field2_finish(
            f,
            "ResultIterator",
            "begin",
            &self.begin,
            "end",
            &&self.end,
        )
    }
}
#[automatically_derived]
impl ::core::marker::Copy for ResultIterator {}
#[automatically_derived]
impl ::core::clone::Clone for ResultIterator {
    #[inline]
    fn clone(&self) -> ResultIterator {
        let _: ::core::clone::AssertParamIsClone<*mut ::std::os::raw::c_void>;
        let _: ::core::clone::AssertParamIsClone<*mut ::std::os::raw::c_void>;
        *self
    }
}
#[allow(clippy::unnecessary_operation, clippy::identity_op)]
const _: () = {
    ["Size of ResultIterator"][::std::mem::size_of::<ResultIterator>() - 16usize];
    ["Alignment of ResultIterator"][::std::mem::align_of::<ResultIterator>() - 8usize];
    [
        "Offset of field: ResultIterator::begin",
    ][{ builtin # offset_of(ResultIterator, begin) } - 0usize];
    [
        "Offset of field: ResultIterator::end",
    ][{ builtin # offset_of(ResultIterator, end) } - 8usize];
};
extern "C" {
    pub fn hfst_empty_transducer() -> *mut ::std::os::raw::c_void;
}
extern "C" {
    pub fn hfst_input_stream(
        path: *const ::std::os::raw::c_char,
    ) -> *mut ::std::os::raw::c_void;
}
extern "C" {
    pub fn hfst_input_stream_close(arg1: *const ::std::os::raw::c_void);
}
extern "C" {
    pub fn hfst_input_stream_is_eof(arg1: *const ::std::os::raw::c_void) -> bool;
}
extern "C" {
    pub fn hfst_input_stream_is_bad(arg1: *const ::std::os::raw::c_void) -> bool;
}
extern "C" {
    pub fn hfst_transducer_from_stream(
        arg1: *const ::std::os::raw::c_void,
    ) -> *mut ::std::os::raw::c_void;
}
extern "C" {
    pub fn hfst_lookup(
        handle: *mut ::std::os::raw::c_void,
        input: *const ::std::os::raw::c_char,
    ) -> *mut ::std::os::raw::c_void;
}
extern "C" {
    pub fn hfst_lookup_iterator(
        arg1: *mut ::std::os::raw::c_void,
    ) -> *mut ResultIterator;
}
extern "C" {
    pub fn hfst_lookup_iterator_value(
        it: *mut ResultIterator,
        s: *mut *mut ::std::os::raw::c_char,
        w: *mut f32,
    );
}
extern "C" {
    pub fn hfst_lookup_iterator_next(it: *mut ResultIterator);
}
extern "C" {
    pub fn hfst_lookup_iterator_free(it: *mut ResultIterator);
}
extern "C" {
    pub fn hfst_lookup_iterator_done(it: *mut ResultIterator) -> bool;
}
