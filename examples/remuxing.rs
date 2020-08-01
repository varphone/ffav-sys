/*
 * Copyright (c) 2013 Stefano Sabatini
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
 * libavformat/libavcodec demuxing and muxing API example.
 *
 * Remux streams from one container format to another.
 * @example remuxing.rs
 */
use ffav_sys::{AVMediaType::*, *};
use std::convert::TryInto;
use std::env;
use std::ffi::CString;

unsafe extern "C" fn log_packet(fmt_ctx: *const AVFormatContext, pkt: *const AVPacket, tag: &str) {
    let pkt = &*pkt;
    let fmt_ctx = &*fmt_ctx;
    let streams: &[*mut AVStream] =
        std::slice::from_raw_parts(fmt_ctx.streams, fmt_ctx.nb_streams as usize);
    let stream = &*streams[pkt.stream_index as usize];
    let time_base = &stream.time_base;

    println!(
        "{}: pts:{} pts_time:{} dts:{} dts_time:{} duration:{} duration_time:{} stream_index:{}",
        tag,
        av_ts2str(pkt.pts),
        av_ts2timestr(pkt.pts, time_base),
        av_ts2str(pkt.dts),
        av_ts2timestr(pkt.dts, time_base),
        av_ts2str(pkt.duration),
        av_ts2timestr(pkt.duration, time_base),
        pkt.stream_index,
    );
}

fn main() {
    unsafe {
        let mut ofmt_ptr: *mut AVOutputFormat = std::ptr::null_mut();
        let mut ifmt_ctx_ptr: *mut AVFormatContext = std::ptr::null_mut();
        let mut ofmt_ctx_ptr: *mut AVFormatContext = std::ptr::null_mut();
        let mut pkt: AVPacket = std::mem::zeroed();
        let mut ret;

        let args = env::args().collect::<Vec<_>>();
        if args.len() < 3 {
            println!(
                "usage: {} input output\n\
                API example program to remux a media file with libavformat and libavcodec.\n\
                The output format is guessed according to the file extension.\n",
                args[0]
            );
            std::process::exit(-1);
        }

        let in_filename = CString::new(args[1].clone()).unwrap();
        let out_filename = CString::new(args[2].clone()).unwrap();

        'outer: loop {
            ret = avformat_open_input(
                &mut ifmt_ctx_ptr,
                in_filename.as_ptr(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if ret < 0 {
                println!("Could not open input file {:?}", in_filename);
                break 'outer;
            }

            ret = avformat_find_stream_info(ifmt_ctx_ptr, std::ptr::null_mut());
            if ret < 0 {
                println!("Failed to retrieve input stream information");
                break 'outer;
            }

            av_dump_format(ifmt_ctx_ptr, 0, in_filename.as_ptr(), 0);

            avformat_alloc_output_context2(
                &mut ofmt_ctx_ptr,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                out_filename.as_ptr(),
            );
            if ofmt_ctx_ptr.is_null() {
                println!("Could not create output context");
                ret = AVERROR_UNKNOWN;
                break 'outer;
            }

            let ifmt_ctx = &mut *ifmt_ctx_ptr;
            let ofmt_ctx = &mut *ofmt_ctx_ptr;

            let in_nb_streams = ifmt_ctx.nb_streams as usize;
            let in_streams: &[*mut AVStream] =
                std::slice::from_raw_parts(ifmt_ctx.streams, in_nb_streams);

            let mut stream_index = 0;
            let mut stream_mapping: Vec<i32> = Vec::with_capacity(in_nb_streams);
            stream_mapping.resize(stream_mapping.capacity(), -1);

            ofmt_ptr = ofmt_ctx.oformat;

            for i in 0..in_nb_streams {
                let in_stream = &mut *in_streams[i];
                let in_codecpar_ptr: *mut AVCodecParameters = in_stream.codecpar;
                let in_codecpar = &mut *in_codecpar_ptr;

                if in_codecpar.codec_type != AVMEDIA_TYPE_AUDIO
                    && in_codecpar.codec_type != AVMEDIA_TYPE_VIDEO
                    && in_codecpar.codec_type != AVMEDIA_TYPE_SUBTITLE
                {
                    stream_mapping[i] = -1;
                    continue;
                }

                stream_mapping[i] = stream_index;
                stream_index += 1;

                let out_stream_ptr = avformat_new_stream(ofmt_ctx_ptr, std::ptr::null_mut());
                if out_stream_ptr.is_null() {
                    println!("Failed allocating output stream");
                    ret = AVERROR_UNKNOWN;
                    break 'outer;
                }

                let out_stream = &mut *out_stream_ptr;

                ret = avcodec_parameters_copy(out_stream.codecpar, in_codecpar_ptr);
                if ret < 0 {
                    println!("Failed to copy codec parameters");
                    break 'outer;
                }
                (*out_stream.codecpar).codec_tag = 0;
            }

            av_dump_format(ofmt_ctx_ptr, 0, out_filename.as_ptr(), 1);

            if ((*ofmt_ptr).flags & AVFMT_NOFILE) != AVFMT_NOFILE {
                ret = avio_open(&mut ofmt_ctx.pb, out_filename.as_ptr(), AVIO_FLAG_WRITE);
                if ret < 0 {
                    println!("Could not open output file {:?}", out_filename);
                    break 'outer;
                }
            }

            ret = avformat_write_header(ofmt_ctx_ptr, std::ptr::null_mut());
            if ret < 0 {
                println!("Error occurred when opening output file");
                break 'outer;
            }

            let out_streams = std::slice::from_raw_parts(
                ofmt_ctx.streams,
                ofmt_ctx.nb_streams.try_into().unwrap(),
            );

            let mut cur_pts: [i64; 64] = [0; 64];

            'inner: loop {
                ret = av_read_frame(ifmt_ctx_ptr, &mut pkt);
                if ret < 0 {
                    break 'inner;
                }

                let curr_stream_index = pkt.stream_index as usize;
                let in_stream_ptr = in_streams[curr_stream_index];
                if curr_stream_index >= stream_mapping.len()
                    || stream_mapping[curr_stream_index] < 0
                {
                    av_packet_unref(&mut pkt);
                    continue;
                }

                pkt.stream_index = stream_mapping[curr_stream_index];
                let out_stream_ptr = out_streams[curr_stream_index];

                let in_stream = &mut *in_stream_ptr;
                let out_stream = &mut *out_stream_ptr;

                let orig_pts = pkt.pts;
                let orig_duration = pkt.duration;

                if orig_pts == AV_NOPTS_VALUE {
                    pkt.pts = cur_pts[curr_stream_index];
                    pkt.dts = pkt.pts;
                }

                log_packet(ifmt_ctx_ptr, &pkt, "in");

                /* copy packet */
                pkt.pts = av_rescale_q_rnd(
                    pkt.pts,
                    in_stream.time_base,
                    out_stream.time_base,
                    AVRounding::new().near_inf().pass_min_max(),
                );
                pkt.dts = av_rescale_q_rnd(
                    pkt.dts,
                    in_stream.time_base,
                    out_stream.time_base,
                    AVRounding::new().near_inf().pass_min_max(),
                );
                pkt.duration =
                    av_rescale_q(pkt.duration, in_stream.time_base, out_stream.time_base);
                pkt.pos = -1;
                log_packet(ofmt_ctx_ptr, &pkt, "out");

                ret = av_interleaved_write_frame(ofmt_ctx_ptr, &mut pkt);
                if ret < 0 {
                    println!("Error muxing packet");
                    break 'inner;
                }

                if orig_pts == AV_NOPTS_VALUE {
                    cur_pts[curr_stream_index] += orig_duration;
                }

                av_packet_unref(&mut pkt);
            }

            av_write_trailer(ofmt_ctx_ptr);

            break 'outer;
        }

        avformat_close_input(&mut ifmt_ctx_ptr);

        // close output
        if !ofmt_ctx_ptr.is_null() && ((*ofmt_ptr).flags & AVFMT_NOFILE) != AVFMT_NOFILE {
            avio_closep(&mut (*ofmt_ctx_ptr).pb);
        }
        avformat_free_context(ofmt_ctx_ptr);

        if ret < 0 && ret != AVERROR_EOF {
            println!("Error occurred: {:?}", av_err2str(ret));
            std::process::exit(-2);
        }
    }
}
