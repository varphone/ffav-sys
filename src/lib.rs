#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(improper_ctypes)]
#![allow(clippy::approx_constant)]
#![allow(clippy::redundant_static_lifetimes)]
#![allow(clippy::type_complexity)]

extern crate libc;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

mod avcodec;
pub use avcodec::*;

mod avformat;
pub use avformat::*;

mod avutil;
pub use avutil::*;
