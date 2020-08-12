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
 * libavformat AVFormatContext API example.
 *
 * Make libavformat demuxer read all packets and
 * write to another file with a little frame size header.
 * @example dump_framed.rs
 */
use ffav_sys::*;
use std::convert::TryInto;
use std::env;
use std::fs::OpenOptions;
use std::io::prelude::*;

#[allow(unused_assignments)]
fn main() {
    unsafe {
        let args = env::args().collect::<Vec<_>>();
        if args.len() != 3 {
            println!(
                "usage: {} <input_file> <output_file>\n\
                API example program to show how to read all packets and \
                write to another file with a little frame size header.",
                args[0]
            );
            std::process::exit(-1);
        }

        let input_filename = std::ffi::CString::new(args[1].clone()).unwrap();
        let mut output = OpenOptions::new()
            .write(true)
            .create(true)
            .open(&args[2])
            .unwrap();

        let mut ctx: *mut AVFormatContext = std::ptr::null_mut();
        let mut ret: i32 = 0;

        'outer: loop {
            ret = avformat_open_input(
                &mut ctx,
                input_filename.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if ret < 0 {
                println!("Could not open input!");
                break 'outer;
            }

            ret = avformat_find_stream_info(ctx, std::ptr::null_mut());
            if ret < 0 {
                println!("Could not find stream information!");
                break 'outer;
            }

            av_dump_format(ctx, 0, input_filename.as_ptr(), 0);

            let mut pkt = AVPacket::default();
            av_init_packet(&mut pkt);

            while av_read_frame(ctx, &mut pkt) >= 0 {
                output.write(&pkt.size.to_be_bytes()).unwrap();
                output
                    .write(std::slice::from_raw_parts(
                        pkt.data,
                        pkt.size.try_into().unwrap(),
                    ))
                    .unwrap();
                av_free_packet(&mut pkt);
            }

            println!("Output (framed) to: '{}'", args[2]);

            break 'outer;
        }

        avformat_close_input(&mut ctx);

        if ret < 0 {
            println!("Error occurred: {:?}", av_err2str(ret));
            std::process::exit(-2);
        }
    }
}
