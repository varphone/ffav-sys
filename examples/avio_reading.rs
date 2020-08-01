/*
 * Copyright (c) 2014 Stefano Sabatini
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
 * libavformat AVIOContext API example.
 *
 * Make libavformat demuxer access media content through a custom
 * AVIOContext read callback.
 * @example avio_reading.rs
 */
use ffav_sys::*;
use libc::{c_void, ENOMEM};
use std::convert::TryInto;
use std::env;

struct BufferData {
    ptr: *mut u8,
    size: usize, // size left in the buffer
}

unsafe extern "C" fn read_packet(opaque: *mut c_void, buf: *mut u8, buf_size: i32) -> i32 {
    let bd = &mut *(opaque as *mut BufferData);
    let buf_size = buf_size.min(bd.size as i32);

    if buf_size == 0 {
        return AVERROR_EOF;
    }

    println!("ptr:{:p} size:{:?}", bd.ptr, bd.size);

    // copy internal buffer data to buf
    std::ptr::copy(bd.ptr, buf, buf_size as usize);
    bd.ptr = bd.ptr.offset(buf_size as isize);
    bd.size -= buf_size as usize;

    return buf_size;
}

#[allow(unused_assignments)]
fn main() {
    unsafe {
        let mut avio_ctx: *mut AVIOContext = std::ptr::null_mut();
        let mut avio_ctx_buffer: *mut u8 = std::ptr::null_mut();
        let avio_ctx_buffer_size: usize = 4096;
        let mut buffer: *mut u8 = std::ptr::null_mut();
        let mut buffer_size: usize = 0;
        let mut fmt_ctx: *mut AVFormatContext = std::ptr::null_mut();
        let mut ret: i32 = 0;
        let mut bd = BufferData {
            ptr: std::ptr::null_mut(),
            size: 0,
        };

        let args = env::args().collect::<Vec<_>>();
        if args.len() != 2 {
            println!(
                "usage: {} input_file\n\
                API example program to show how to read from a custom buffer \
                accessed through AVIOContext.",
                args[0]
            );
            std::process::exit(-1);
        }

        let input_filename = std::ffi::CString::new(args[1].clone()).unwrap();

        loop {
            // slurp file content into buffer
            ret = av_file_map(
                input_filename.as_ptr(),
                &mut buffer,
                &mut buffer_size,
                0,
                std::ptr::null_mut(),
            );
            if ret < 0 {
                break;
            }

            // fill opaque structure used by the AVIOContext read callback
            bd.ptr = buffer;
            bd.size = buffer_size;

            fmt_ctx = avformat_alloc_context();
            if fmt_ctx.is_null() {
                ret = AVERROR(ENOMEM);
                break;
            }

            avio_ctx_buffer = av_malloc(avio_ctx_buffer_size) as *mut u8;
            if avio_ctx_buffer.is_null() {
                ret = AVERROR(ENOMEM);
                break;
            }
            avio_ctx = avio_alloc_context(
                avio_ctx_buffer,
                avio_ctx_buffer_size.try_into().unwrap(),
                0,
                &mut bd as *mut BufferData as *mut c_void,
                Some(read_packet),
                None,
                None,
            );
            if avio_ctx.is_null() {
                ret = AVERROR(ENOMEM);
                break;
            }
            (*fmt_ctx).pb = avio_ctx;

            ret = avformat_open_input(
                &mut fmt_ctx,
                std::ptr::null(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if ret < 0 {
                println!("Could not open input!");
                break;
            }

            ret = avformat_find_stream_info(fmt_ctx, std::ptr::null_mut());
            if ret < 0 {
                println!("Could not find stream information!");
                break;
            }

            av_dump_format(fmt_ctx, 0, input_filename.as_ptr(), 0);

            break;
        }

        avformat_close_input(&mut fmt_ctx);

        // note: the internal buffer could have changed, and be != avio_ctx_buffer
        if !avio_ctx.is_null() {
            av_freep(std::mem::transmute::<*mut *mut u8, *mut c_void>(
                &mut (*avio_ctx).buffer,
            ));
        }
        avio_context_free(&mut avio_ctx);

        av_file_unmap(buffer, buffer_size);

        if ret < 0 {
            println!("Error occurred: {:?}", av_err2str(ret));
            std::process::exit(-2);
        }
    }
}
