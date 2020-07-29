/*
 * Copyright (c) 2011 Reinhard Tartler
 * Copyright (c) 2020 Varphone Wong
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in
 * all copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL
 * THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
 * THE SOFTWARE.
 */

/**
 * @file
 * Shows how the metadata API can be used in application programs.
 * @example metadata.rs
 */
use ffi::{
    av_dict_get, avformat_close_input, avformat_find_stream_info, avformat_open_input,
    AVDictionaryEntry, AVFormatContext, AV_DICT_IGNORE_SUFFIX,
};
use ffmpeg_sys_next as ffi;
use std::env;
use std::ffi::{CStr, CString};

fn main() {
    unsafe {
        let mut fmt_ctx: *mut AVFormatContext = std::ptr::null_mut();
        let mut tag: *mut AVDictionaryEntry = std::ptr::null_mut();
        let mut ret;

        let args = env::args().collect::<Vec<_>>();
        if args.len() != 2 {
            println!(
                "usage: {} input_file\n\
                example program to demonstrate the use of the libavformat metadata API.",
                args[0]
            );
            std::process::exit(-1);
        }

        let input_filename = CString::new(args[1].clone()).unwrap();
        ret = avformat_open_input(
            &mut fmt_ctx,
            input_filename.as_ptr(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        if ret != 0 {
            panic!("avformat_open_input({:?}) = {}", input_filename, ret);
        }

        ret = avformat_find_stream_info(fmt_ctx, std::ptr::null_mut());
        if ret < 0 {
            panic!("avformat_find_stream_info({:?}) = {}", fmt_ctx, ret);
        }

        let dummy = CString::new("").unwrap();
        loop {
            tag = av_dict_get(
                (*fmt_ctx).metadata,
                dummy.as_ptr(),
                tag,
                AV_DICT_IGNORE_SUFFIX,
            );
            if tag.is_null() {
                break;
            }
            println!(
                "{:?}={:?}",
                CStr::from_ptr((*tag).key),
                CStr::from_ptr((*tag).value)
            );
        }

        avformat_close_input(&mut fmt_ctx);
    }
}
