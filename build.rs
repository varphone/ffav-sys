extern crate bindgen;
extern crate cc;
extern crate num_cpus;
extern crate pkg_config;
extern crate regex;

use std::env;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;
use std::str;

use bindgen::callbacks::{
    EnumVariantCustomBehavior, EnumVariantValue, IntKind, MacroParsingBehavior, ParseCallbacks,
};
use regex::Regex;

#[derive(Debug)]
struct Library {
    name: &'static str,
    is_feature: bool,
}

impl Library {
    fn feature_name(&self) -> Option<String> {
        if self.is_feature {
            Some("CARGO_FEATURE_".to_string() + &self.name.to_uppercase())
        } else {
            None
        }
    }
}

static LIBRARIES: &[Library] = &[
    Library {
        name: "avcodec",
        is_feature: true,
    },
    Library {
        name: "avdevice",
        is_feature: true,
    },
    Library {
        name: "avfilter",
        is_feature: true,
    },
    Library {
        name: "avformat",
        is_feature: true,
    },
    Library {
        name: "avresample",
        is_feature: true,
    },
    Library {
        name: "avutil",
        is_feature: false,
    },
    Library {
        name: "postproc",
        is_feature: true,
    },
    Library {
        name: "swresample",
        is_feature: true,
    },
    Library {
        name: "swscale",
        is_feature: true,
    },
];

#[derive(Debug)]
struct Callbacks;

impl ParseCallbacks for Callbacks {
    #[allow(clippy::trivial_regex)]
    fn int_macro(&self, _name: &str, value: i64) -> Option<IntKind> {
        let ch_layout = Regex::new(r"^AV_CH").unwrap();
        let codec_cap = Regex::new(r"^AV_CODEC_CAP").unwrap();
        let codec_flag = Regex::new(r"^AV_CODEC_FLAG").unwrap();
        let error_max_size = Regex::new(r"^AV_ERROR_MAX_STRING_SIZE").unwrap();

        if value >= i64::min_value() as i64
            && value <= i64::max_value() as i64
            && ch_layout.is_match(_name)
        {
            Some(IntKind::ULongLong)
        } else if value >= i32::min_value() as i64
            && value <= i32::max_value() as i64
            && (codec_cap.is_match(_name) || codec_flag.is_match(_name))
        {
            Some(IntKind::UInt)
        } else if error_max_size.is_match(_name) {
            Some(IntKind::Custom {
                name: "usize",
                is_signed: false,
            })
        } else if value >= i32::min_value() as i64 && value <= i32::max_value() as i64 {
            Some(IntKind::Int)
        } else {
            None
        }
    }

    #[allow(clippy::trivial_regex)]
    fn enum_variant_behavior(
        &self,
        _enum_name: Option<&str>,
        original_variant_name: &str,
        _variant_value: EnumVariantValue,
    ) -> Option<EnumVariantCustomBehavior> {
        let dummy_codec_id = Regex::new(r"^AV_CODEC_ID_FIRST").unwrap();
        if dummy_codec_id.is_match(original_variant_name) {
            Some(EnumVariantCustomBehavior::Constify)
        } else {
            None
        }
    }

    // https://github.com/rust-lang/rust-bindgen/issues/687#issuecomment-388277405
    fn will_parse_macro(&self, name: &str) -> MacroParsingBehavior {
        use MacroParsingBehavior::*;

        match name {
            "FP_INFINITE" => Ignore,
            "FP_NAN" => Ignore,
            "FP_NORMAL" => Ignore,
            "FP_SUBNORMAL" => Ignore,
            "FP_ZERO" => Ignore,
            _ => Default,
        }
    }
}

fn version() -> String {
    let major: u8 = env::var("CARGO_PKG_VERSION_MAJOR")
        .unwrap()
        .parse()
        .unwrap();
    let minor: u8 = env::var("CARGO_PKG_VERSION_MINOR")
        .unwrap()
        .parse()
        .unwrap();

    format!("{}.{}", major, minor)
}

fn output() -> PathBuf {
    PathBuf::from(env::var("OUT_DIR").unwrap())
}

fn source() -> PathBuf {
    output().join(format!("ffmpeg-{}", version()))
}

fn search() -> PathBuf {
    let mut absolute = env::current_dir().unwrap();
    absolute.push(&output());
    absolute.push("dist");

    absolute
}

fn fetch() -> io::Result<()> {
    let configure_path = &output()
        .join(format!("ffmpeg-{}", version()))
        .join("configure");
    if fs::metadata(configure_path).is_ok() {
        return Ok(());
    }
    let url = env::var("FFMPEG_GIT_URL")
        .unwrap_or_else(|_| "https://github.com/FFmpeg/FFmpeg".to_string());
    let status = Command::new("git")
        .current_dir(&output())
        .arg("clone")
        .arg("-b")
        .arg(format!("release/{}", version()))
        .arg(url)
        .arg(format!("ffmpeg-{}", version()))
        .status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::new(io::ErrorKind::Other, "fetch failed"))
    }
}

fn switch(configure: &mut Command, feature: &str, name: &str) {
    let arg = if env::var("CARGO_FEATURE_".to_string() + feature).is_ok() {
        "--enable-"
    } else {
        "--disable-"
    };
    configure.arg(arg.to_string() + name);
}

fn build() -> io::Result<()> {
    let mut configure = Command::new("./configure");
    configure.current_dir(&source());
    configure.arg(format!("--prefix={}", search().to_string_lossy()));

    if env::var("TARGET").unwrap() != env::var("HOST").unwrap() {
        let target = env::var("TARGET").unwrap();
        let linker = env::var("RUSTC_LINKER").unwrap();
        if linker.contains(&target) {
            configure.arg(format!("--cross-prefix={}-", target));
        } else {
            let (target, _) = &linker.split_at(linker.rfind('-').unwrap());
            configure.arg(format!("--cross-prefix={}-", target));
        }
        configure.arg(format!(
            "--arch={}",
            env::var("CARGO_CFG_TARGET_ARCH").unwrap()
        ));
        configure.arg(format!(
            "--target-os={}",
            env::var("CARGO_CFG_TARGET_OS").unwrap()
        ));
    }

    // control debug build
    if env::var("DEBUG").is_ok() {
        configure.arg("--enable-debug");
        configure.arg("--disable-stripping");
    } else {
        configure.arg("--disable-debug");
        configure.arg("--enable-stripping");
    }

    // make it static
    configure.arg("--enable-static");
    configure.arg("--disable-shared");

    configure.arg("--enable-pic");

    // do not build programs since we don't need them
    configure.arg("--disable-programs");

    macro_rules! enable {
        ($conf:expr, $feat:expr, $name:expr) => {
            if env::var(concat!("CARGO_FEATURE_", $feat)).is_ok() {
                $conf.arg(concat!("--enable-", $name));
            }
        };
    }

    // macro_rules! disable {
    //     ($conf:expr, $feat:expr, $name:expr) => (
    //         if env::var(concat!("CARGO_FEATURE_", $feat)).is_err() {
    //             $conf.arg(concat!("--disable-", $name));
    //         }
    //     )
    // }

    // the binary using ffmpeg-sys must comply with GPL
    switch(&mut configure, "BUILD_LICENSE_GPL", "gpl");

    // the binary using ffmpeg-sys must comply with (L)GPLv3
    switch(&mut configure, "BUILD_LICENSE_VERSION3", "version3");

    // the binary using ffmpeg-sys cannot be redistributed
    switch(&mut configure, "BUILD_LICENSE_NONFREE", "nonfree");

    // configure building libraries based on features
    for lib in LIBRARIES.iter().filter(|lib| lib.is_feature) {
        switch(&mut configure, &lib.name.to_uppercase(), lib.name);
    }

    // configure external SSL libraries
    enable!(configure, "BUILD_LIB_GNUTLS", "gnutls");
    enable!(configure, "BUILD_LIB_OPENSSL", "openssl");

    // configure external filters
    enable!(configure, "BUILD_LIB_FONTCONFIG", "fontconfig");
    enable!(configure, "BUILD_LIB_FREI0R", "frei0r");
    enable!(configure, "BUILD_LIB_LADSPA", "ladspa");
    enable!(configure, "BUILD_LIB_ASS", "libass");
    enable!(configure, "BUILD_LIB_FREETYPE", "libfreetype");
    enable!(configure, "BUILD_LIB_FRIBIDI", "libfribidi");
    enable!(configure, "BUILD_LIB_OPENCV", "libopencv");

    // configure external encoders/decoders
    enable!(configure, "BUILD_LIB_AACPLUS", "libaacplus");
    enable!(configure, "BUILD_LIB_CELT", "libcelt");
    enable!(configure, "BUILD_LIB_DCADEC", "libdcadec");
    enable!(configure, "BUILD_LIB_FAAC", "libfaac");
    enable!(configure, "BUILD_LIB_FDK_AAC", "libfdk-aac");
    enable!(configure, "BUILD_LIB_GSM", "libgsm");
    enable!(configure, "BUILD_LIB_ILBC", "libilbc");
    enable!(configure, "BUILD_LIB_VAZAAR", "libvazaar");
    enable!(configure, "BUILD_LIB_MP3LAME", "libmp3lame");
    enable!(configure, "BUILD_LIB_OPENCORE_AMRNB", "libopencore-amrnb");
    enable!(configure, "BUILD_LIB_OPENCORE_AMRWB", "libopencore-amrwb");
    enable!(configure, "BUILD_LIB_OPENH264", "libopenh264");
    enable!(configure, "BUILD_LIB_OPENH265", "libopenh265");
    enable!(configure, "BUILD_LIB_OPENJPEG", "libopenjpeg");
    enable!(configure, "BUILD_LIB_OPUS", "libopus");
    enable!(configure, "BUILD_LIB_SCHROEDINGER", "libschroedinger");
    enable!(configure, "BUILD_LIB_SHINE", "libshine");
    enable!(configure, "BUILD_LIB_SNAPPY", "libsnappy");
    enable!(configure, "BUILD_LIB_SPEEX", "libspeex");
    enable!(
        configure,
        "BUILD_LIB_STAGEFRIGHT_H264",
        "libstagefright-h264"
    );
    enable!(configure, "BUILD_LIB_THEORA", "libtheora");
    enable!(configure, "BUILD_LIB_TWOLAME", "libtwolame");
    enable!(configure, "BUILD_LIB_UTVIDEO", "libutvideo");
    enable!(configure, "BUILD_LIB_VO_AACENC", "libvo-aacenc");
    enable!(configure, "BUILD_LIB_VO_AMRWBENC", "libvo-amrwbenc");
    enable!(configure, "BUILD_LIB_VORBIS", "libvorbis");
    enable!(configure, "BUILD_LIB_VPX", "libvpx");
    enable!(configure, "BUILD_LIB_WAVPACK", "libwavpack");
    enable!(configure, "BUILD_LIB_WEBP", "libwebp");
    enable!(configure, "BUILD_LIB_X264", "libx264");
    enable!(configure, "BUILD_LIB_X265", "libx265");
    enable!(configure, "BUILD_LIB_AVS", "libavs");
    enable!(configure, "BUILD_LIB_XVID", "libxvid");

    // other external libraries
    enable!(configure, "BUILD_NVENC", "nvenc");

    // configure external protocols
    enable!(configure, "BUILD_LIB_SMBCLIENT", "libsmbclient");
    enable!(configure, "BUILD_LIB_SSH", "libssh");

    // configure misc build options
    enable!(configure, "BUILD_PIC", "pic");

    // configure every components
    if env::var("CARGO_FEATURE_DISABLE_EVERYTHING").is_ok() {
        configure.arg("--disable-everything");
    }

    // configure bsfs
    if env::var("CARGO_FEATURE_DISABLE_BSFS").is_ok() {
        configure.arg("--disable-bsfs");

        macro_rules! enable_bsf {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_BSF_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut bsfs: Vec<&str> = vec![];
        enable_bsf!(bsfs, "aac_adtstoasc");
        enable_bsf!(bsfs, "av1_frame_merge");
        enable_bsf!(bsfs, "av1_frame_split");
        enable_bsf!(bsfs, "av1_metadata");
        enable_bsf!(bsfs, "chomp");
        enable_bsf!(bsfs, "dca_core");
        enable_bsf!(bsfs, "dump_extradata");
        enable_bsf!(bsfs, "eac3_core");
        enable_bsf!(bsfs, "extract_extradata");
        enable_bsf!(bsfs, "filter_units");
        enable_bsf!(bsfs, "h264_metadata");
        enable_bsf!(bsfs, "h264_mp4toannexb");
        enable_bsf!(bsfs, "h264_redundant_pps");
        enable_bsf!(bsfs, "hapqa_extract");
        enable_bsf!(bsfs, "hevc_metadata");
        enable_bsf!(bsfs, "hevc_mp4toannexb");
        enable_bsf!(bsfs, "imx_dump_header");
        enable_bsf!(bsfs, "mjpeg2jpeg");
        enable_bsf!(bsfs, "mjpega_dump_header");
        enable_bsf!(bsfs, "mov2textsub");
        enable_bsf!(bsfs, "mp3_header_decompress");
        enable_bsf!(bsfs, "mpeg2_metadata");
        enable_bsf!(bsfs, "mpeg4_unpack_bframes");
        enable_bsf!(bsfs, "noise");
        enable_bsf!(bsfs, "null");
        enable_bsf!(bsfs, "opus_metadata");
        enable_bsf!(bsfs, "pcm_rechunk");
        enable_bsf!(bsfs, "prores_metadata");
        enable_bsf!(bsfs, "remove_extradata");
        enable_bsf!(bsfs, "text2movsub");
        enable_bsf!(bsfs, "trace_headers");
        enable_bsf!(bsfs, "truehd_core");
        enable_bsf!(bsfs, "vp9_metadata");
        enable_bsf!(bsfs, "vp9_raw_reorder");
        enable_bsf!(bsfs, "vp9_superframe");
        enable_bsf!(bsfs, "vp9_superframe_split");

        if !bsfs.is_empty() {
            configure.arg(format!("--enable-bsf={}", bsfs.join(",")));
        }
    }

    // configure decoders
    if env::var("CARGO_FEATURE_DISABLE_DECODERS").is_ok() {
        configure.arg("--disable-decoders");

        macro_rules! enable_decoder {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_DECODER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut decoders: Vec<&str> = vec![];
        enable_decoder!(decoders, "aac");
        enable_decoder!(decoders, "aac_at");
        enable_decoder!(decoders, "aac_fixed");
        enable_decoder!(decoders, "aac_latm");
        enable_decoder!(decoders, "aasc");
        enable_decoder!(decoders, "ac3");
        enable_decoder!(decoders, "ac3_at");
        enable_decoder!(decoders, "ac3_fixed");
        enable_decoder!(decoders, "acelp_kelvin");
        enable_decoder!(decoders, "adpcm_4xm");
        enable_decoder!(decoders, "adpcm_adx");
        enable_decoder!(decoders, "adpcm_afc");
        enable_decoder!(decoders, "adpcm_agm");
        enable_decoder!(decoders, "adpcm_aica");
        enable_decoder!(decoders, "adpcm_argo");
        enable_decoder!(decoders, "adpcm_ct");
        enable_decoder!(decoders, "adpcm_dtk");
        enable_decoder!(decoders, "adpcm_ea");
        enable_decoder!(decoders, "adpcm_ea_maxis_xa");
        enable_decoder!(decoders, "adpcm_ea_r1");
        enable_decoder!(decoders, "adpcm_ea_r2");
        enable_decoder!(decoders, "adpcm_ea_r3");
        enable_decoder!(decoders, "adpcm_ea_xas");
        enable_decoder!(decoders, "adpcm_g722");
        enable_decoder!(decoders, "adpcm_g726");
        enable_decoder!(decoders, "adpcm_g726le");
        enable_decoder!(decoders, "adpcm_ima_alp");
        enable_decoder!(decoders, "adpcm_ima_amv");
        enable_decoder!(decoders, "adpcm_ima_apc");
        enable_decoder!(decoders, "adpcm_ima_apm");
        enable_decoder!(decoders, "adpcm_ima_cunning");
        enable_decoder!(decoders, "adpcm_ima_dat4");
        enable_decoder!(decoders, "adpcm_ima_dk3");
        enable_decoder!(decoders, "adpcm_ima_dk4");
        enable_decoder!(decoders, "adpcm_ima_ea_eacs");
        enable_decoder!(decoders, "adpcm_ima_ea_sead");
        enable_decoder!(decoders, "adpcm_ima_iss");
        enable_decoder!(decoders, "adpcm_ima_mtf");
        enable_decoder!(decoders, "adpcm_ima_oki");
        enable_decoder!(decoders, "adpcm_ima_qt");
        enable_decoder!(decoders, "adpcm_ima_qt_at");
        enable_decoder!(decoders, "adpcm_ima_rad");
        enable_decoder!(decoders, "adpcm_ima_smjpeg");
        enable_decoder!(decoders, "adpcm_ima_ssi");
        enable_decoder!(decoders, "adpcm_ima_wav");
        enable_decoder!(decoders, "adpcm_ima_ws");
        enable_decoder!(decoders, "adpcm_ms");
        enable_decoder!(decoders, "adpcm_mtaf");
        enable_decoder!(decoders, "adpcm_psx");
        enable_decoder!(decoders, "adpcm_sbpro_2");
        enable_decoder!(decoders, "adpcm_sbpro_3");
        enable_decoder!(decoders, "adpcm_sbpro_4");
        enable_decoder!(decoders, "adpcm_swf");
        enable_decoder!(decoders, "adpcm_thp");
        enable_decoder!(decoders, "adpcm_thp_le");
        enable_decoder!(decoders, "adpcm_vima");
        enable_decoder!(decoders, "adpcm_xa");
        enable_decoder!(decoders, "adpcm_yamaha");
        enable_decoder!(decoders, "adpcm_zork");
        enable_decoder!(decoders, "agm");
        enable_decoder!(decoders, "aic");
        enable_decoder!(decoders, "alac");
        enable_decoder!(decoders, "alac_at");
        enable_decoder!(decoders, "alias_pix");
        enable_decoder!(decoders, "als");
        enable_decoder!(decoders, "amr_nb_at");
        enable_decoder!(decoders, "amrnb");
        enable_decoder!(decoders, "amrwb");
        enable_decoder!(decoders, "amv");
        enable_decoder!(decoders, "anm");
        enable_decoder!(decoders, "ansi");
        enable_decoder!(decoders, "ape");
        enable_decoder!(decoders, "apng");
        enable_decoder!(decoders, "aptx");
        enable_decoder!(decoders, "aptx_hd");
        enable_decoder!(decoders, "arbc");
        enable_decoder!(decoders, "ass");
        enable_decoder!(decoders, "asv1");
        enable_decoder!(decoders, "asv2");
        enable_decoder!(decoders, "atrac1");
        enable_decoder!(decoders, "atrac3");
        enable_decoder!(decoders, "atrac3al");
        enable_decoder!(decoders, "atrac3p");
        enable_decoder!(decoders, "atrac3pal");
        enable_decoder!(decoders, "atrac9");
        enable_decoder!(decoders, "aura");
        enable_decoder!(decoders, "aura2");
        enable_decoder!(decoders, "avrn");
        enable_decoder!(decoders, "avrp");
        enable_decoder!(decoders, "avs");
        enable_decoder!(decoders, "avui");
        enable_decoder!(decoders, "ayuv");
        enable_decoder!(decoders, "bethsoftvid");
        enable_decoder!(decoders, "bfi");
        enable_decoder!(decoders, "bink");
        enable_decoder!(decoders, "binkaudio_dct");
        enable_decoder!(decoders, "binkaudio_rdft");
        enable_decoder!(decoders, "bintext");
        enable_decoder!(decoders, "bitpacked");
        enable_decoder!(decoders, "bmp");
        enable_decoder!(decoders, "bmv_audio");
        enable_decoder!(decoders, "bmv_video");
        enable_decoder!(decoders, "brender_pix");
        enable_decoder!(decoders, "c93");
        enable_decoder!(decoders, "cavs");
        enable_decoder!(decoders, "ccaption");
        enable_decoder!(decoders, "cdgraphics");
        enable_decoder!(decoders, "cdtoons");
        enable_decoder!(decoders, "cdxl");
        enable_decoder!(decoders, "cfhd");
        enable_decoder!(decoders, "cinepak");
        enable_decoder!(decoders, "clearvideo");
        enable_decoder!(decoders, "cljr");
        enable_decoder!(decoders, "cllc");
        enable_decoder!(decoders, "comfortnoise");
        enable_decoder!(decoders, "cook");
        enable_decoder!(decoders, "cpia");
        enable_decoder!(decoders, "cscd");
        enable_decoder!(decoders, "cyuv");
        enable_decoder!(decoders, "dca");
        enable_decoder!(decoders, "dds");
        enable_decoder!(decoders, "derf_dpcm");
        enable_decoder!(decoders, "dfa");
        enable_decoder!(decoders, "dirac");
        enable_decoder!(decoders, "dnxhd");
        enable_decoder!(decoders, "dolby_e");
        enable_decoder!(decoders, "dpx");
        enable_decoder!(decoders, "dsd_lsbf");
        enable_decoder!(decoders, "dsd_lsbf_planar");
        enable_decoder!(decoders, "dsd_msbf");
        enable_decoder!(decoders, "dsd_msbf_planar");
        enable_decoder!(decoders, "dsicinaudio");
        enable_decoder!(decoders, "dsicinvideo");
        enable_decoder!(decoders, "dss_sp");
        enable_decoder!(decoders, "dst");
        enable_decoder!(decoders, "dvaudio");
        enable_decoder!(decoders, "dvbsub");
        enable_decoder!(decoders, "dvdsub");
        enable_decoder!(decoders, "dvvideo");
        enable_decoder!(decoders, "dxa");
        enable_decoder!(decoders, "dxtory");
        enable_decoder!(decoders, "dxv");
        enable_decoder!(decoders, "eac3");
        enable_decoder!(decoders, "eac3_at");
        enable_decoder!(decoders, "eacmv");
        enable_decoder!(decoders, "eamad");
        enable_decoder!(decoders, "eatgq");
        enable_decoder!(decoders, "eatgv");
        enable_decoder!(decoders, "eatqi");
        enable_decoder!(decoders, "eightbps");
        enable_decoder!(decoders, "eightsvx_exp");
        enable_decoder!(decoders, "eightsvx_fib");
        enable_decoder!(decoders, "escape124");
        enable_decoder!(decoders, "escape130");
        enable_decoder!(decoders, "evrc");
        enable_decoder!(decoders, "exr");
        enable_decoder!(decoders, "ffv1");
        enable_decoder!(decoders, "ffvhuff");
        enable_decoder!(decoders, "ffwavesynth");
        enable_decoder!(decoders, "fic");
        enable_decoder!(decoders, "fits");
        enable_decoder!(decoders, "flac");
        enable_decoder!(decoders, "flashsv");
        enable_decoder!(decoders, "flashsv2");
        enable_decoder!(decoders, "flic");
        enable_decoder!(decoders, "flv");
        enable_decoder!(decoders, "fmvc");
        enable_decoder!(decoders, "fourxm");
        enable_decoder!(decoders, "fraps");
        enable_decoder!(decoders, "frwu");
        enable_decoder!(decoders, "g2m");
        enable_decoder!(decoders, "g723_1");
        enable_decoder!(decoders, "g729");
        enable_decoder!(decoders, "gdv");
        enable_decoder!(decoders, "gif");
        enable_decoder!(decoders, "gremlin_dpcm");
        enable_decoder!(decoders, "gsm");
        enable_decoder!(decoders, "gsm_ms");
        enable_decoder!(decoders, "gsm_ms_at");
        enable_decoder!(decoders, "h261");
        enable_decoder!(decoders, "h263");
        enable_decoder!(decoders, "h263_v4l2m2m");
        enable_decoder!(decoders, "h263i");
        enable_decoder!(decoders, "h263p");
        enable_decoder!(decoders, "h264");
        enable_decoder!(decoders, "h264_crystalhd");
        enable_decoder!(decoders, "h264_cuvid");
        enable_decoder!(decoders, "h264_mediacodec");
        enable_decoder!(decoders, "h264_mmal");
        enable_decoder!(decoders, "h264_qsv");
        enable_decoder!(decoders, "h264_rkmpp");
        enable_decoder!(decoders, "h264_v4l2m2m");
        enable_decoder!(decoders, "hap");
        enable_decoder!(decoders, "hca");
        enable_decoder!(decoders, "hcom");
        enable_decoder!(decoders, "hevc");
        enable_decoder!(decoders, "hevc_cuvid");
        enable_decoder!(decoders, "hevc_mediacodec");
        enable_decoder!(decoders, "hevc_qsv");
        enable_decoder!(decoders, "hevc_rkmpp");
        enable_decoder!(decoders, "hevc_v4l2m2m");
        enable_decoder!(decoders, "hnm4_video");
        enable_decoder!(decoders, "hq_hqa");
        enable_decoder!(decoders, "hqx");
        enable_decoder!(decoders, "huffyuv");
        enable_decoder!(decoders, "hymt");
        enable_decoder!(decoders, "iac");
        enable_decoder!(decoders, "idcin");
        enable_decoder!(decoders, "idf");
        enable_decoder!(decoders, "iff_ilbm");
        enable_decoder!(decoders, "ilbc");
        enable_decoder!(decoders, "ilbc_at");
        enable_decoder!(decoders, "imc");
        enable_decoder!(decoders, "imm4");
        enable_decoder!(decoders, "imm5");
        enable_decoder!(decoders, "indeo2");
        enable_decoder!(decoders, "indeo3");
        enable_decoder!(decoders, "indeo4");
        enable_decoder!(decoders, "indeo5");
        enable_decoder!(decoders, "interplay_acm");
        enable_decoder!(decoders, "interplay_dpcm");
        enable_decoder!(decoders, "interplay_video");
        enable_decoder!(decoders, "jacosub");
        enable_decoder!(decoders, "jpeg2000");
        enable_decoder!(decoders, "jpegls");
        enable_decoder!(decoders, "jv");
        enable_decoder!(decoders, "kgv1");
        enable_decoder!(decoders, "kmvc");
        enable_decoder!(decoders, "lagarith");
        enable_decoder!(decoders, "libaom_av1");
        enable_decoder!(decoders, "libaribb24");
        enable_decoder!(decoders, "libcelt");
        enable_decoder!(decoders, "libcodec2");
        enable_decoder!(decoders, "libdav1d");
        enable_decoder!(decoders, "libdavs2");
        enable_decoder!(decoders, "libfdk_aac");
        enable_decoder!(decoders, "libgsm");
        enable_decoder!(decoders, "libgsm_ms");
        enable_decoder!(decoders, "libilbc");
        enable_decoder!(decoders, "libopencore_amrnb");
        enable_decoder!(decoders, "libopencore_amrwb");
        enable_decoder!(decoders, "libopenh264");
        enable_decoder!(decoders, "libopenjpeg");
        enable_decoder!(decoders, "libopus");
        enable_decoder!(decoders, "librsvg");
        enable_decoder!(decoders, "libspeex");
        enable_decoder!(decoders, "libvorbis");
        enable_decoder!(decoders, "libvpx_vp8");
        enable_decoder!(decoders, "libvpx_vp9");
        enable_decoder!(decoders, "libzvbi_teletext");
        enable_decoder!(decoders, "loco");
        enable_decoder!(decoders, "lscr");
        enable_decoder!(decoders, "m101");
        enable_decoder!(decoders, "mace3");
        enable_decoder!(decoders, "mace6");
        enable_decoder!(decoders, "magicyuv");
        enable_decoder!(decoders, "mdec");
        enable_decoder!(decoders, "metasound");
        enable_decoder!(decoders, "microdvd");
        enable_decoder!(decoders, "mimic");
        enable_decoder!(decoders, "mjpeg");
        enable_decoder!(decoders, "mjpeg_cuvid");
        enable_decoder!(decoders, "mjpeg_qsv");
        enable_decoder!(decoders, "mjpegb");
        enable_decoder!(decoders, "mlp");
        enable_decoder!(decoders, "mmvideo");
        enable_decoder!(decoders, "motionpixels");
        enable_decoder!(decoders, "movtext");
        enable_decoder!(decoders, "mp1");
        enable_decoder!(decoders, "mp1_at");
        enable_decoder!(decoders, "mp1float");
        enable_decoder!(decoders, "mp2");
        enable_decoder!(decoders, "mp2_at");
        enable_decoder!(decoders, "mp2float");
        enable_decoder!(decoders, "mp3");
        enable_decoder!(decoders, "mp3_at");
        enable_decoder!(decoders, "mp3adu");
        enable_decoder!(decoders, "mp3adufloat");
        enable_decoder!(decoders, "mp3float");
        enable_decoder!(decoders, "mp3on4");
        enable_decoder!(decoders, "mp3on4float");
        enable_decoder!(decoders, "mpc7");
        enable_decoder!(decoders, "mpc8");
        enable_decoder!(decoders, "mpeg1_cuvid");
        enable_decoder!(decoders, "mpeg1_v4l2m2m");
        enable_decoder!(decoders, "mpeg1video");
        enable_decoder!(decoders, "mpeg2_crystalhd");
        enable_decoder!(decoders, "mpeg2_cuvid");
        enable_decoder!(decoders, "mpeg2_mediacodec");
        enable_decoder!(decoders, "mpeg2_mmal");
        enable_decoder!(decoders, "mpeg2_qsv");
        enable_decoder!(decoders, "mpeg2_v4l2m2m");
        enable_decoder!(decoders, "mpeg2video");
        enable_decoder!(decoders, "mpeg4");
        enable_decoder!(decoders, "mpeg4_crystalhd");
        enable_decoder!(decoders, "mpeg4_cuvid");
        enable_decoder!(decoders, "mpeg4_mediacodec");
        enable_decoder!(decoders, "mpeg4_mmal");
        enable_decoder!(decoders, "mpeg4_v4l2m2m");
        enable_decoder!(decoders, "mpegvideo");
        enable_decoder!(decoders, "mpl2");
        enable_decoder!(decoders, "msa1");
        enable_decoder!(decoders, "mscc");
        enable_decoder!(decoders, "msmpeg4_crystalhd");
        enable_decoder!(decoders, "msmpeg4v1");
        enable_decoder!(decoders, "msmpeg4v2");
        enable_decoder!(decoders, "msmpeg4v3");
        enable_decoder!(decoders, "msrle");
        enable_decoder!(decoders, "mss1");
        enable_decoder!(decoders, "mss2");
        enable_decoder!(decoders, "msvideo1");
        enable_decoder!(decoders, "mszh");
        enable_decoder!(decoders, "mts2");
        enable_decoder!(decoders, "mv30");
        enable_decoder!(decoders, "mvc1");
        enable_decoder!(decoders, "mvc2");
        enable_decoder!(decoders, "mvdv");
        enable_decoder!(decoders, "mvha");
        enable_decoder!(decoders, "mwsc");
        enable_decoder!(decoders, "mxpeg");
        enable_decoder!(decoders, "nellymoser");
        enable_decoder!(decoders, "notchlc");
        enable_decoder!(decoders, "nuv");
        enable_decoder!(decoders, "on2avc");
        enable_decoder!(decoders, "opus");
        enable_decoder!(decoders, "paf_audio");
        enable_decoder!(decoders, "paf_video");
        enable_decoder!(decoders, "pam");
        enable_decoder!(decoders, "pbm");
        enable_decoder!(decoders, "pcm_alaw");
        enable_decoder!(decoders, "pcm_alaw_at");
        enable_decoder!(decoders, "pcm_bluray");
        enable_decoder!(decoders, "pcm_dvd");
        enable_decoder!(decoders, "pcm_f16le");
        enable_decoder!(decoders, "pcm_f24le");
        enable_decoder!(decoders, "pcm_f32be");
        enable_decoder!(decoders, "pcm_f32le");
        enable_decoder!(decoders, "pcm_f64be");
        enable_decoder!(decoders, "pcm_f64le");
        enable_decoder!(decoders, "pcm_lxf");
        enable_decoder!(decoders, "pcm_mulaw");
        enable_decoder!(decoders, "pcm_mulaw_at");
        enable_decoder!(decoders, "pcm_s16be");
        enable_decoder!(decoders, "pcm_s16be_planar");
        enable_decoder!(decoders, "pcm_s16le");
        enable_decoder!(decoders, "pcm_s16le_planar");
        enable_decoder!(decoders, "pcm_s24be");
        enable_decoder!(decoders, "pcm_s24daud");
        enable_decoder!(decoders, "pcm_s24le");
        enable_decoder!(decoders, "pcm_s24le_planar");
        enable_decoder!(decoders, "pcm_s32be");
        enable_decoder!(decoders, "pcm_s32le");
        enable_decoder!(decoders, "pcm_s32le_planar");
        enable_decoder!(decoders, "pcm_s64be");
        enable_decoder!(decoders, "pcm_s64le");
        enable_decoder!(decoders, "pcm_s8");
        enable_decoder!(decoders, "pcm_s8_planar");
        enable_decoder!(decoders, "pcm_u16be");
        enable_decoder!(decoders, "pcm_u16le");
        enable_decoder!(decoders, "pcm_u24be");
        enable_decoder!(decoders, "pcm_u24le");
        enable_decoder!(decoders, "pcm_u32be");
        enable_decoder!(decoders, "pcm_u32le");
        enable_decoder!(decoders, "pcm_u8");
        enable_decoder!(decoders, "pcm_vidc");
        enable_decoder!(decoders, "pcx");
        enable_decoder!(decoders, "pfm");
        enable_decoder!(decoders, "pgm");
        enable_decoder!(decoders, "pgmyuv");
        enable_decoder!(decoders, "pgssub");
        enable_decoder!(decoders, "pgx");
        enable_decoder!(decoders, "pictor");
        enable_decoder!(decoders, "pixlet");
        enable_decoder!(decoders, "pjs");
        enable_decoder!(decoders, "png");
        enable_decoder!(decoders, "ppm");
        enable_decoder!(decoders, "prores");
        enable_decoder!(decoders, "prosumer");
        enable_decoder!(decoders, "psd");
        enable_decoder!(decoders, "ptx");
        enable_decoder!(decoders, "qcelp");
        enable_decoder!(decoders, "qdm2");
        enable_decoder!(decoders, "qdm2_at");
        enable_decoder!(decoders, "qdmc");
        enable_decoder!(decoders, "qdmc_at");
        enable_decoder!(decoders, "qdraw");
        enable_decoder!(decoders, "qpeg");
        enable_decoder!(decoders, "qtrle");
        enable_decoder!(decoders, "r10k");
        enable_decoder!(decoders, "r210");
        enable_decoder!(decoders, "ra_144");
        enable_decoder!(decoders, "ra_288");
        enable_decoder!(decoders, "ralf");
        enable_decoder!(decoders, "rasc");
        enable_decoder!(decoders, "rawvideo");
        enable_decoder!(decoders, "realtext");
        enable_decoder!(decoders, "rl2");
        enable_decoder!(decoders, "roq");
        enable_decoder!(decoders, "roq_dpcm");
        enable_decoder!(decoders, "rpza");
        enable_decoder!(decoders, "rscc");
        enable_decoder!(decoders, "rv10");
        enable_decoder!(decoders, "rv20");
        enable_decoder!(decoders, "rv30");
        enable_decoder!(decoders, "rv40");
        enable_decoder!(decoders, "s302m");
        enable_decoder!(decoders, "sami");
        enable_decoder!(decoders, "sanm");
        enable_decoder!(decoders, "sbc");
        enable_decoder!(decoders, "scpr");
        enable_decoder!(decoders, "screenpresso");
        enable_decoder!(decoders, "sdx2_dpcm");
        enable_decoder!(decoders, "sgi");
        enable_decoder!(decoders, "sgirle");
        enable_decoder!(decoders, "sheervideo");
        enable_decoder!(decoders, "shorten");
        enable_decoder!(decoders, "sipr");
        enable_decoder!(decoders, "siren");
        enable_decoder!(decoders, "smackaud");
        enable_decoder!(decoders, "smacker");
        enable_decoder!(decoders, "smc");
        enable_decoder!(decoders, "smvjpeg");
        enable_decoder!(decoders, "snow");
        enable_decoder!(decoders, "sol_dpcm");
        enable_decoder!(decoders, "sonic");
        enable_decoder!(decoders, "sp5x");
        enable_decoder!(decoders, "speedhq");
        enable_decoder!(decoders, "srgc");
        enable_decoder!(decoders, "srt");
        enable_decoder!(decoders, "ssa");
        enable_decoder!(decoders, "stl");
        enable_decoder!(decoders, "subrip");
        enable_decoder!(decoders, "subviewer");
        enable_decoder!(decoders, "subviewer1");
        enable_decoder!(decoders, "sunrast");
        enable_decoder!(decoders, "svq1");
        enable_decoder!(decoders, "svq3");
        enable_decoder!(decoders, "tak");
        enable_decoder!(decoders, "targa");
        enable_decoder!(decoders, "targa_y216");
        enable_decoder!(decoders, "tdsc");
        enable_decoder!(decoders, "text");
        enable_decoder!(decoders, "theora");
        enable_decoder!(decoders, "thp");
        enable_decoder!(decoders, "tiertexseqvideo");
        enable_decoder!(decoders, "tiff");
        enable_decoder!(decoders, "tmv");
        enable_decoder!(decoders, "truehd");
        enable_decoder!(decoders, "truemotion1");
        enable_decoder!(decoders, "truemotion2");
        enable_decoder!(decoders, "truemotion2rt");
        enable_decoder!(decoders, "truespeech");
        enable_decoder!(decoders, "tscc");
        enable_decoder!(decoders, "tscc2");
        enable_decoder!(decoders, "tta");
        enable_decoder!(decoders, "twinvq");
        enable_decoder!(decoders, "txd");
        enable_decoder!(decoders, "ulti");
        enable_decoder!(decoders, "utvideo");
        enable_decoder!(decoders, "v210");
        enable_decoder!(decoders, "v210x");
        enable_decoder!(decoders, "v308");
        enable_decoder!(decoders, "v408");
        enable_decoder!(decoders, "v410");
        enable_decoder!(decoders, "vb");
        enable_decoder!(decoders, "vble");
        enable_decoder!(decoders, "vc1");
        enable_decoder!(decoders, "vc1_crystalhd");
        enable_decoder!(decoders, "vc1_cuvid");
        enable_decoder!(decoders, "vc1_mmal");
        enable_decoder!(decoders, "vc1_qsv");
        enable_decoder!(decoders, "vc1_v4l2m2m");
        enable_decoder!(decoders, "vc1image");
        enable_decoder!(decoders, "vcr1");
        enable_decoder!(decoders, "vmdaudio");
        enable_decoder!(decoders, "vmdvideo");
        enable_decoder!(decoders, "vmnc");
        enable_decoder!(decoders, "vorbis");
        enable_decoder!(decoders, "vp3");
        enable_decoder!(decoders, "vp4");
        enable_decoder!(decoders, "vp5");
        enable_decoder!(decoders, "vp6");
        enable_decoder!(decoders, "vp6a");
        enable_decoder!(decoders, "vp6f");
        enable_decoder!(decoders, "vp7");
        enable_decoder!(decoders, "vp8");
        enable_decoder!(decoders, "vp8_cuvid");
        enable_decoder!(decoders, "vp8_mediacodec");
        enable_decoder!(decoders, "vp8_qsv");
        enable_decoder!(decoders, "vp8_rkmpp");
        enable_decoder!(decoders, "vp8_v4l2m2m");
        enable_decoder!(decoders, "vp9");
        enable_decoder!(decoders, "vp9_cuvid");
        enable_decoder!(decoders, "vp9_mediacodec");
        enable_decoder!(decoders, "vp9_qsv");
        enable_decoder!(decoders, "vp9_rkmpp");
        enable_decoder!(decoders, "vp9_v4l2m2m");
        enable_decoder!(decoders, "vplayer");
        enable_decoder!(decoders, "vqa");
        enable_decoder!(decoders, "wavpack");
        enable_decoder!(decoders, "wcmv");
        enable_decoder!(decoders, "webp");
        enable_decoder!(decoders, "webvtt");
        enable_decoder!(decoders, "wmalossless");
        enable_decoder!(decoders, "wmapro");
        enable_decoder!(decoders, "wmav1");
        enable_decoder!(decoders, "wmav2");
        enable_decoder!(decoders, "wmavoice");
        enable_decoder!(decoders, "wmv1");
        enable_decoder!(decoders, "wmv2");
        enable_decoder!(decoders, "wmv3");
        enable_decoder!(decoders, "wmv3_crystalhd");
        enable_decoder!(decoders, "wmv3image");
        enable_decoder!(decoders, "wnv1");
        enable_decoder!(decoders, "wrapped_avframe");
        enable_decoder!(decoders, "ws_snd1");
        enable_decoder!(decoders, "xan_dpcm");
        enable_decoder!(decoders, "xan_wc3");
        enable_decoder!(decoders, "xan_wc4");
        enable_decoder!(decoders, "xbin");
        enable_decoder!(decoders, "xbm");
        enable_decoder!(decoders, "xface");
        enable_decoder!(decoders, "xl");
        enable_decoder!(decoders, "xma1");
        enable_decoder!(decoders, "xma2");
        enable_decoder!(decoders, "xpm");
        enable_decoder!(decoders, "xsub");
        enable_decoder!(decoders, "xwd");
        enable_decoder!(decoders, "y41p");
        enable_decoder!(decoders, "ylc");
        enable_decoder!(decoders, "yop");
        enable_decoder!(decoders, "yuv4");
        enable_decoder!(decoders, "zero12v");
        enable_decoder!(decoders, "zerocodec");
        enable_decoder!(decoders, "zlib");
        enable_decoder!(decoders, "zmbv");

        if !decoders.is_empty() {
            configure.arg(format!("--enable-decoder={}", decoders.join(",")));
        }
    }

    // configure demuxers
    if env::var("CARGO_FEATURE_DISABLE_DEMUXERS").is_ok() {
        configure.arg("--disable-demuxers");

        macro_rules! enable_demuxer {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_DEMUXER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut demuxers: Vec<&str> = vec![];
        enable_demuxer!(demuxers, "aa");
        enable_demuxer!(demuxers, "aac");
        enable_demuxer!(demuxers, "ac3");
        enable_demuxer!(demuxers, "acm");
        enable_demuxer!(demuxers, "act");
        enable_demuxer!(demuxers, "adf");
        enable_demuxer!(demuxers, "adp");
        enable_demuxer!(demuxers, "ads");
        enable_demuxer!(demuxers, "adx");
        enable_demuxer!(demuxers, "aea");
        enable_demuxer!(demuxers, "afc");
        enable_demuxer!(demuxers, "aiff");
        enable_demuxer!(demuxers, "aix");
        enable_demuxer!(demuxers, "alp");
        enable_demuxer!(demuxers, "amr");
        enable_demuxer!(demuxers, "amrnb");
        enable_demuxer!(demuxers, "amrwb");
        enable_demuxer!(demuxers, "anm");
        enable_demuxer!(demuxers, "apc");
        enable_demuxer!(demuxers, "ape");
        enable_demuxer!(demuxers, "apm");
        enable_demuxer!(demuxers, "apng");
        enable_demuxer!(demuxers, "aptx");
        enable_demuxer!(demuxers, "aptx_hd");
        enable_demuxer!(demuxers, "aqtitle");
        enable_demuxer!(demuxers, "argo_asf");
        enable_demuxer!(demuxers, "asf");
        enable_demuxer!(demuxers, "asf_o");
        enable_demuxer!(demuxers, "ass");
        enable_demuxer!(demuxers, "ast");
        enable_demuxer!(demuxers, "au");
        enable_demuxer!(demuxers, "av1");
        enable_demuxer!(demuxers, "avi");
        enable_demuxer!(demuxers, "avisynth");
        enable_demuxer!(demuxers, "avr");
        enable_demuxer!(demuxers, "avs");
        enable_demuxer!(demuxers, "avs2");
        enable_demuxer!(demuxers, "bethsoftvid");
        enable_demuxer!(demuxers, "bfi");
        enable_demuxer!(demuxers, "bfstm");
        enable_demuxer!(demuxers, "bink");
        enable_demuxer!(demuxers, "bintext");
        enable_demuxer!(demuxers, "bit");
        enable_demuxer!(demuxers, "bmv");
        enable_demuxer!(demuxers, "boa");
        enable_demuxer!(demuxers, "brstm");
        enable_demuxer!(demuxers, "c93");
        enable_demuxer!(demuxers, "caf");
        enable_demuxer!(demuxers, "cavsvideo");
        enable_demuxer!(demuxers, "cdg");
        enable_demuxer!(demuxers, "cdxl");
        enable_demuxer!(demuxers, "cine");
        enable_demuxer!(demuxers, "codec2");
        enable_demuxer!(demuxers, "codec2raw");
        enable_demuxer!(demuxers, "concat");
        enable_demuxer!(demuxers, "dash");
        enable_demuxer!(demuxers, "data");
        enable_demuxer!(demuxers, "daud");
        enable_demuxer!(demuxers, "dcstr");
        enable_demuxer!(demuxers, "derf");
        enable_demuxer!(demuxers, "dfa");
        enable_demuxer!(demuxers, "dhav");
        enable_demuxer!(demuxers, "dirac");
        enable_demuxer!(demuxers, "dnxhd");
        enable_demuxer!(demuxers, "dsf");
        enable_demuxer!(demuxers, "dsicin");
        enable_demuxer!(demuxers, "dss");
        enable_demuxer!(demuxers, "dts");
        enable_demuxer!(demuxers, "dtshd");
        enable_demuxer!(demuxers, "dv");
        enable_demuxer!(demuxers, "dvbsub");
        enable_demuxer!(demuxers, "dvbtxt");
        enable_demuxer!(demuxers, "dxa");
        enable_demuxer!(demuxers, "ea");
        enable_demuxer!(demuxers, "ea_cdata");
        enable_demuxer!(demuxers, "eac3");
        enable_demuxer!(demuxers, "epaf");
        enable_demuxer!(demuxers, "ffmetadata");
        enable_demuxer!(demuxers, "filmstrip");
        enable_demuxer!(demuxers, "fits");
        enable_demuxer!(demuxers, "flac");
        enable_demuxer!(demuxers, "flic");
        enable_demuxer!(demuxers, "flv");
        enable_demuxer!(demuxers, "fourxm");
        enable_demuxer!(demuxers, "frm");
        enable_demuxer!(demuxers, "fsb");
        enable_demuxer!(demuxers, "fwse");
        enable_demuxer!(demuxers, "g722");
        enable_demuxer!(demuxers, "g723_1");
        enable_demuxer!(demuxers, "g726");
        enable_demuxer!(demuxers, "g726le");
        enable_demuxer!(demuxers, "g729");
        enable_demuxer!(demuxers, "gdv");
        enable_demuxer!(demuxers, "genh");
        enable_demuxer!(demuxers, "gif");
        enable_demuxer!(demuxers, "gsm");
        enable_demuxer!(demuxers, "gxf");
        enable_demuxer!(demuxers, "h261");
        enable_demuxer!(demuxers, "h263");
        enable_demuxer!(demuxers, "h264");
        enable_demuxer!(demuxers, "hca");
        enable_demuxer!(demuxers, "hcom");
        enable_demuxer!(demuxers, "hevc");
        enable_demuxer!(demuxers, "hls");
        enable_demuxer!(demuxers, "hnm");
        enable_demuxer!(demuxers, "ico");
        enable_demuxer!(demuxers, "idcin");
        enable_demuxer!(demuxers, "idf");
        enable_demuxer!(demuxers, "iff");
        enable_demuxer!(demuxers, "ifv");
        enable_demuxer!(demuxers, "ilbc");
        enable_demuxer!(demuxers, "image_bmp_pipe");
        enable_demuxer!(demuxers, "image_dds_pipe");
        enable_demuxer!(demuxers, "image_dpx_pipe");
        enable_demuxer!(demuxers, "image_exr_pipe");
        enable_demuxer!(demuxers, "image_gif_pipe");
        enable_demuxer!(demuxers, "image_j2k_pipe");
        enable_demuxer!(demuxers, "image_jpeg_pipe");
        enable_demuxer!(demuxers, "image_jpegls_pipe");
        enable_demuxer!(demuxers, "image_pam_pipe");
        enable_demuxer!(demuxers, "image_pbm_pipe");
        enable_demuxer!(demuxers, "image_pcx_pipe");
        enable_demuxer!(demuxers, "image_pgm_pipe");
        enable_demuxer!(demuxers, "image_pgmyuv_pipe");
        enable_demuxer!(demuxers, "image_pgx_pipe");
        enable_demuxer!(demuxers, "image_pictor_pipe");
        enable_demuxer!(demuxers, "image_png_pipe");
        enable_demuxer!(demuxers, "image_ppm_pipe");
        enable_demuxer!(demuxers, "image_psd_pipe");
        enable_demuxer!(demuxers, "image_qdraw_pipe");
        enable_demuxer!(demuxers, "image_sgi_pipe");
        enable_demuxer!(demuxers, "image_sunrast_pipe");
        enable_demuxer!(demuxers, "image_svg_pipe");
        enable_demuxer!(demuxers, "image_tiff_pipe");
        enable_demuxer!(demuxers, "image_webp_pipe");
        enable_demuxer!(demuxers, "image_xpm_pipe");
        enable_demuxer!(demuxers, "image_xwd_pipe");
        enable_demuxer!(demuxers, "image2");
        enable_demuxer!(demuxers, "image2_alias_pix");
        enable_demuxer!(demuxers, "image2_brender_pix");
        enable_demuxer!(demuxers, "image2pipe");
        enable_demuxer!(demuxers, "ingenient");
        enable_demuxer!(demuxers, "ipmovie");
        enable_demuxer!(demuxers, "ircam");
        enable_demuxer!(demuxers, "iss");
        enable_demuxer!(demuxers, "iv8");
        enable_demuxer!(demuxers, "ivf");
        enable_demuxer!(demuxers, "ivr");
        enable_demuxer!(demuxers, "jacosub");
        enable_demuxer!(demuxers, "jv");
        enable_demuxer!(demuxers, "kux");
        enable_demuxer!(demuxers, "kvag");
        enable_demuxer!(demuxers, "libgme");
        enable_demuxer!(demuxers, "libmodplug");
        enable_demuxer!(demuxers, "libopenmpt");
        enable_demuxer!(demuxers, "live_flv");
        enable_demuxer!(demuxers, "lmlm4");
        enable_demuxer!(demuxers, "loas");
        enable_demuxer!(demuxers, "lrc");
        enable_demuxer!(demuxers, "lvf");
        enable_demuxer!(demuxers, "lxf");
        enable_demuxer!(demuxers, "m4v");
        enable_demuxer!(demuxers, "matroska");
        enable_demuxer!(demuxers, "mcc");
        enable_demuxer!(demuxers, "mgsts");
        enable_demuxer!(demuxers, "microdvd");
        enable_demuxer!(demuxers, "mjpeg");
        enable_demuxer!(demuxers, "mjpeg_2000");
        enable_demuxer!(demuxers, "mlp");
        enable_demuxer!(demuxers, "mlv");
        enable_demuxer!(demuxers, "mm");
        enable_demuxer!(demuxers, "mmf");
        enable_demuxer!(demuxers, "mov");
        enable_demuxer!(demuxers, "mp3");
        enable_demuxer!(demuxers, "mpc");
        enable_demuxer!(demuxers, "mpc8");
        enable_demuxer!(demuxers, "mpegps");
        enable_demuxer!(demuxers, "mpegts");
        enable_demuxer!(demuxers, "mpegtsraw");
        enable_demuxer!(demuxers, "mpegvideo");
        enable_demuxer!(demuxers, "mpjpeg");
        enable_demuxer!(demuxers, "mpl2");
        enable_demuxer!(demuxers, "mpsub");
        enable_demuxer!(demuxers, "msf");
        enable_demuxer!(demuxers, "msnwc_tcp");
        enable_demuxer!(demuxers, "mtaf");
        enable_demuxer!(demuxers, "mtv");
        enable_demuxer!(demuxers, "musx");
        enable_demuxer!(demuxers, "mv");
        enable_demuxer!(demuxers, "mvi");
        enable_demuxer!(demuxers, "mxf");
        enable_demuxer!(demuxers, "mxg");
        enable_demuxer!(demuxers, "nc");
        enable_demuxer!(demuxers, "nistsphere");
        enable_demuxer!(demuxers, "nsp");
        enable_demuxer!(demuxers, "nsv");
        enable_demuxer!(demuxers, "nut");
        enable_demuxer!(demuxers, "nuv");
        enable_demuxer!(demuxers, "ogg");
        enable_demuxer!(demuxers, "oma");
        enable_demuxer!(demuxers, "paf");
        enable_demuxer!(demuxers, "pcm_alaw");
        enable_demuxer!(demuxers, "pcm_f32be");
        enable_demuxer!(demuxers, "pcm_f32le");
        enable_demuxer!(demuxers, "pcm_f64be");
        enable_demuxer!(demuxers, "pcm_f64le");
        enable_demuxer!(demuxers, "pcm_mulaw");
        enable_demuxer!(demuxers, "pcm_s16be");
        enable_demuxer!(demuxers, "pcm_s16le");
        enable_demuxer!(demuxers, "pcm_s24be");
        enable_demuxer!(demuxers, "pcm_s24le");
        enable_demuxer!(demuxers, "pcm_s32be");
        enable_demuxer!(demuxers, "pcm_s32le");
        enable_demuxer!(demuxers, "pcm_s8");
        enable_demuxer!(demuxers, "pcm_u16be");
        enable_demuxer!(demuxers, "pcm_u16le");
        enable_demuxer!(demuxers, "pcm_u24be");
        enable_demuxer!(demuxers, "pcm_u24le");
        enable_demuxer!(demuxers, "pcm_u32be");
        enable_demuxer!(demuxers, "pcm_u32le");
        enable_demuxer!(demuxers, "pcm_u8");
        enable_demuxer!(demuxers, "pcm_vidc");
        enable_demuxer!(demuxers, "pjs");
        enable_demuxer!(demuxers, "pmp");
        enable_demuxer!(demuxers, "pp_bnk");
        enable_demuxer!(demuxers, "pva");
        enable_demuxer!(demuxers, "pvf");
        enable_demuxer!(demuxers, "qcp");
        enable_demuxer!(demuxers, "r3d");
        enable_demuxer!(demuxers, "rawvideo");
        enable_demuxer!(demuxers, "realtext");
        enable_demuxer!(demuxers, "redspark");
        enable_demuxer!(demuxers, "rl2");
        enable_demuxer!(demuxers, "rm");
        enable_demuxer!(demuxers, "roq");
        enable_demuxer!(demuxers, "rpl");
        enable_demuxer!(demuxers, "rsd");
        enable_demuxer!(demuxers, "rso");
        enable_demuxer!(demuxers, "rtp");
        enable_demuxer!(demuxers, "rtsp");
        enable_demuxer!(demuxers, "s337m");
        enable_demuxer!(demuxers, "sami");
        enable_demuxer!(demuxers, "sap");
        enable_demuxer!(demuxers, "sbc");
        enable_demuxer!(demuxers, "sbg");
        enable_demuxer!(demuxers, "scc");
        enable_demuxer!(demuxers, "sdp");
        enable_demuxer!(demuxers, "sdr2");
        enable_demuxer!(demuxers, "sds");
        enable_demuxer!(demuxers, "sdx");
        enable_demuxer!(demuxers, "segafilm");
        enable_demuxer!(demuxers, "ser");
        enable_demuxer!(demuxers, "shorten");
        enable_demuxer!(demuxers, "siff");
        enable_demuxer!(demuxers, "sln");
        enable_demuxer!(demuxers, "smacker");
        enable_demuxer!(demuxers, "smjpeg");
        enable_demuxer!(demuxers, "smush");
        enable_demuxer!(demuxers, "sol");
        enable_demuxer!(demuxers, "sox");
        enable_demuxer!(demuxers, "spdif");
        enable_demuxer!(demuxers, "srt");
        enable_demuxer!(demuxers, "stl");
        enable_demuxer!(demuxers, "str");
        enable_demuxer!(demuxers, "subviewer");
        enable_demuxer!(demuxers, "subviewer1");
        enable_demuxer!(demuxers, "sup");
        enable_demuxer!(demuxers, "svag");
        enable_demuxer!(demuxers, "swf");
        enable_demuxer!(demuxers, "tak");
        enable_demuxer!(demuxers, "tedcaptions");
        enable_demuxer!(demuxers, "thp");
        enable_demuxer!(demuxers, "threedostr");
        enable_demuxer!(demuxers, "tiertexseq");
        enable_demuxer!(demuxers, "tmv");
        enable_demuxer!(demuxers, "truehd");
        enable_demuxer!(demuxers, "tta");
        enable_demuxer!(demuxers, "tty");
        enable_demuxer!(demuxers, "txd");
        enable_demuxer!(demuxers, "ty");
        enable_demuxer!(demuxers, "v210");
        enable_demuxer!(demuxers, "v210x");
        enable_demuxer!(demuxers, "vag");
        enable_demuxer!(demuxers, "vapoursynth");
        enable_demuxer!(demuxers, "vc1");
        enable_demuxer!(demuxers, "vc1t");
        enable_demuxer!(demuxers, "vividas");
        enable_demuxer!(demuxers, "vivo");
        enable_demuxer!(demuxers, "vmd");
        enable_demuxer!(demuxers, "vobsub");
        enable_demuxer!(demuxers, "voc");
        enable_demuxer!(demuxers, "vpk");
        enable_demuxer!(demuxers, "vplayer");
        enable_demuxer!(demuxers, "vqf");
        enable_demuxer!(demuxers, "w64");
        enable_demuxer!(demuxers, "wav");
        enable_demuxer!(demuxers, "wc3");
        enable_demuxer!(demuxers, "webm_dash_manifest");
        enable_demuxer!(demuxers, "webvtt");
        enable_demuxer!(demuxers, "wsaud");
        enable_demuxer!(demuxers, "wsd");
        enable_demuxer!(demuxers, "wsvqa");
        enable_demuxer!(demuxers, "wtv");
        enable_demuxer!(demuxers, "wv");
        enable_demuxer!(demuxers, "wve");
        enable_demuxer!(demuxers, "xa");
        enable_demuxer!(demuxers, "xbin");
        enable_demuxer!(demuxers, "xmv");
        enable_demuxer!(demuxers, "xvag");
        enable_demuxer!(demuxers, "xwma");
        enable_demuxer!(demuxers, "yop");
        enable_demuxer!(demuxers, "yuv4mpegpipe");

        if !demuxers.is_empty() {
            configure.arg(format!("--enable-demuxer={}", demuxers.join(",")));
        }
    }

    // configure encoders
    if env::var("CARGO_FEATURE_DISABLE_ENCODERS").is_ok() {
        configure.arg("--disable-encoders");

        macro_rules! enable_encoder {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_ENCODER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut encoders: Vec<&str> = vec![];
        enable_encoder!(encoders, "a64multi");
        enable_encoder!(encoders, "a64multi5");
        enable_encoder!(encoders, "aac");
        enable_encoder!(encoders, "aac_at");
        enable_encoder!(encoders, "aac_mf");
        enable_encoder!(encoders, "ac3");
        enable_encoder!(encoders, "ac3_fixed");
        enable_encoder!(encoders, "ac3_mf");
        enable_encoder!(encoders, "adpcm_adx");
        enable_encoder!(encoders, "adpcm_g722");
        enable_encoder!(encoders, "adpcm_g726");
        enable_encoder!(encoders, "adpcm_g726le");
        enable_encoder!(encoders, "adpcm_ima_apm");
        enable_encoder!(encoders, "adpcm_ima_qt");
        enable_encoder!(encoders, "adpcm_ima_ssi");
        enable_encoder!(encoders, "adpcm_ima_wav");
        enable_encoder!(encoders, "adpcm_ms");
        enable_encoder!(encoders, "adpcm_swf");
        enable_encoder!(encoders, "adpcm_yamaha");
        enable_encoder!(encoders, "alac");
        enable_encoder!(encoders, "alac_at");
        enable_encoder!(encoders, "alias_pix");
        enable_encoder!(encoders, "amv");
        enable_encoder!(encoders, "apng");
        enable_encoder!(encoders, "aptx");
        enable_encoder!(encoders, "aptx_hd");
        enable_encoder!(encoders, "ass");
        enable_encoder!(encoders, "asv1");
        enable_encoder!(encoders, "asv2");
        enable_encoder!(encoders, "avrp");
        enable_encoder!(encoders, "avui");
        enable_encoder!(encoders, "ayuv");
        enable_encoder!(encoders, "bmp");
        enable_encoder!(encoders, "cinepak");
        enable_encoder!(encoders, "cljr");
        enable_encoder!(encoders, "comfortnoise");
        enable_encoder!(encoders, "dca");
        enable_encoder!(encoders, "dnxhd");
        enable_encoder!(encoders, "dpx");
        enable_encoder!(encoders, "dvbsub");
        enable_encoder!(encoders, "dvdsub");
        enable_encoder!(encoders, "dvvideo");
        enable_encoder!(encoders, "eac3");
        enable_encoder!(encoders, "ffv1");
        enable_encoder!(encoders, "ffvhuff");
        enable_encoder!(encoders, "fits");
        enable_encoder!(encoders, "flac");
        enable_encoder!(encoders, "flashsv");
        enable_encoder!(encoders, "flashsv2");
        enable_encoder!(encoders, "flv");
        enable_encoder!(encoders, "g723_1");
        enable_encoder!(encoders, "gif");
        enable_encoder!(encoders, "h261");
        enable_encoder!(encoders, "h263");
        enable_encoder!(encoders, "h263_v4l2m2m");
        enable_encoder!(encoders, "h263p");
        enable_encoder!(encoders, "h264_amf");
        enable_encoder!(encoders, "h264_mf");
        enable_encoder!(encoders, "h264_nvenc");
        enable_encoder!(encoders, "h264_omx");
        enable_encoder!(encoders, "h264_qsv");
        enable_encoder!(encoders, "h264_v4l2m2m");
        enable_encoder!(encoders, "h264_vaapi");
        enable_encoder!(encoders, "h264_videotoolbox");
        enable_encoder!(encoders, "hap");
        enable_encoder!(encoders, "hevc_amf");
        enable_encoder!(encoders, "hevc_mf");
        enable_encoder!(encoders, "hevc_nvenc");
        enable_encoder!(encoders, "hevc_qsv");
        enable_encoder!(encoders, "hevc_v4l2m2m");
        enable_encoder!(encoders, "hevc_vaapi");
        enable_encoder!(encoders, "hevc_videotoolbox");
        enable_encoder!(encoders, "huffyuv");
        enable_encoder!(encoders, "ilbc_at");
        enable_encoder!(encoders, "jpeg2000");
        enable_encoder!(encoders, "jpegls");
        enable_encoder!(encoders, "libaom_av1");
        enable_encoder!(encoders, "libcodec2");
        enable_encoder!(encoders, "libfdk_aac");
        enable_encoder!(encoders, "libgsm");
        enable_encoder!(encoders, "libgsm_ms");
        enable_encoder!(encoders, "libilbc");
        enable_encoder!(encoders, "libkvazaar");
        enable_encoder!(encoders, "libmp3lame");
        enable_encoder!(encoders, "libopencore_amrnb");
        enable_encoder!(encoders, "libopenh264");
        enable_encoder!(encoders, "libopenjpeg");
        enable_encoder!(encoders, "libopus");
        enable_encoder!(encoders, "librav1e");
        enable_encoder!(encoders, "libshine");
        enable_encoder!(encoders, "libspeex");
        enable_encoder!(encoders, "libtheora");
        enable_encoder!(encoders, "libtwolame");
        enable_encoder!(encoders, "libvo_amrwbenc");
        enable_encoder!(encoders, "libvorbis");
        enable_encoder!(encoders, "libvpx_vp8");
        enable_encoder!(encoders, "libvpx_vp9");
        enable_encoder!(encoders, "libwavpack");
        enable_encoder!(encoders, "libwebp");
        enable_encoder!(encoders, "libwebp_anim");
        enable_encoder!(encoders, "libx262");
        enable_encoder!(encoders, "libx264");
        enable_encoder!(encoders, "libx264rgb");
        enable_encoder!(encoders, "libx265");
        enable_encoder!(encoders, "libxavs");
        enable_encoder!(encoders, "libxavs2");
        enable_encoder!(encoders, "libxvid");
        enable_encoder!(encoders, "ljpeg");
        enable_encoder!(encoders, "magicyuv");
        enable_encoder!(encoders, "mjpeg");
        enable_encoder!(encoders, "mjpeg_qsv");
        enable_encoder!(encoders, "mjpeg_vaapi");
        enable_encoder!(encoders, "mlp");
        enable_encoder!(encoders, "movtext");
        enable_encoder!(encoders, "mp2");
        enable_encoder!(encoders, "mp2fixed");
        enable_encoder!(encoders, "mp3_mf");
        enable_encoder!(encoders, "mpeg1video");
        enable_encoder!(encoders, "mpeg2_qsv");
        enable_encoder!(encoders, "mpeg2_vaapi");
        enable_encoder!(encoders, "mpeg2video");
        enable_encoder!(encoders, "mpeg4");
        enable_encoder!(encoders, "mpeg4_omx");
        enable_encoder!(encoders, "mpeg4_v4l2m2m");
        enable_encoder!(encoders, "msmpeg4v2");
        enable_encoder!(encoders, "msmpeg4v3");
        enable_encoder!(encoders, "msvideo1");
        enable_encoder!(encoders, "nellymoser");
        enable_encoder!(encoders, "nvenc");
        enable_encoder!(encoders, "nvenc_h264");
        enable_encoder!(encoders, "nvenc_hevc");
        enable_encoder!(encoders, "opus");
        enable_encoder!(encoders, "pam");
        enable_encoder!(encoders, "pbm");
        enable_encoder!(encoders, "pcm_alaw");
        enable_encoder!(encoders, "pcm_alaw_at");
        enable_encoder!(encoders, "pcm_dvd");
        enable_encoder!(encoders, "pcm_f32be");
        enable_encoder!(encoders, "pcm_f32le");
        enable_encoder!(encoders, "pcm_f64be");
        enable_encoder!(encoders, "pcm_f64le");
        enable_encoder!(encoders, "pcm_mulaw");
        enable_encoder!(encoders, "pcm_mulaw_at");
        enable_encoder!(encoders, "pcm_s16be");
        enable_encoder!(encoders, "pcm_s16be_planar");
        enable_encoder!(encoders, "pcm_s16le");
        enable_encoder!(encoders, "pcm_s16le_planar");
        enable_encoder!(encoders, "pcm_s24be");
        enable_encoder!(encoders, "pcm_s24daud");
        enable_encoder!(encoders, "pcm_s24le");
        enable_encoder!(encoders, "pcm_s24le_planar");
        enable_encoder!(encoders, "pcm_s32be");
        enable_encoder!(encoders, "pcm_s32le");
        enable_encoder!(encoders, "pcm_s32le_planar");
        enable_encoder!(encoders, "pcm_s64be");
        enable_encoder!(encoders, "pcm_s64le");
        enable_encoder!(encoders, "pcm_s8");
        enable_encoder!(encoders, "pcm_s8_planar");
        enable_encoder!(encoders, "pcm_u16be");
        enable_encoder!(encoders, "pcm_u16le");
        enable_encoder!(encoders, "pcm_u24be");
        enable_encoder!(encoders, "pcm_u24le");
        enable_encoder!(encoders, "pcm_u32be");
        enable_encoder!(encoders, "pcm_u32le");
        enable_encoder!(encoders, "pcm_u8");
        enable_encoder!(encoders, "pcm_vidc");
        enable_encoder!(encoders, "pcx");
        enable_encoder!(encoders, "pgm");
        enable_encoder!(encoders, "pgmyuv");
        enable_encoder!(encoders, "png");
        enable_encoder!(encoders, "ppm");
        enable_encoder!(encoders, "prores");
        enable_encoder!(encoders, "prores_aw");
        enable_encoder!(encoders, "prores_ks");
        enable_encoder!(encoders, "qtrle");
        enable_encoder!(encoders, "r10k");
        enable_encoder!(encoders, "r210");
        enable_encoder!(encoders, "ra_144");
        enable_encoder!(encoders, "rawvideo");
        enable_encoder!(encoders, "roq");
        enable_encoder!(encoders, "roq_dpcm");
        enable_encoder!(encoders, "rv10");
        enable_encoder!(encoders, "rv20");
        enable_encoder!(encoders, "s302m");
        enable_encoder!(encoders, "sbc");
        enable_encoder!(encoders, "sgi");
        enable_encoder!(encoders, "snow");
        enable_encoder!(encoders, "sonic");
        enable_encoder!(encoders, "sonic_ls");
        enable_encoder!(encoders, "srt");
        enable_encoder!(encoders, "ssa");
        enable_encoder!(encoders, "subrip");
        enable_encoder!(encoders, "sunrast");
        enable_encoder!(encoders, "svq1");
        enable_encoder!(encoders, "targa");
        enable_encoder!(encoders, "text");
        enable_encoder!(encoders, "tiff");
        enable_encoder!(encoders, "truehd");
        enable_encoder!(encoders, "tta");
        enable_encoder!(encoders, "utvideo");
        enable_encoder!(encoders, "v210");
        enable_encoder!(encoders, "v308");
        enable_encoder!(encoders, "v408");
        enable_encoder!(encoders, "v410");
        enable_encoder!(encoders, "vc2");
        enable_encoder!(encoders, "vorbis");
        enable_encoder!(encoders, "vp8_v4l2m2m");
        enable_encoder!(encoders, "vp8_vaapi");
        enable_encoder!(encoders, "vp9_qsv");
        enable_encoder!(encoders, "vp9_vaapi");
        enable_encoder!(encoders, "wavpack");
        enable_encoder!(encoders, "webvtt");
        enable_encoder!(encoders, "wmav1");
        enable_encoder!(encoders, "wmav2");
        enable_encoder!(encoders, "wmv1");
        enable_encoder!(encoders, "wmv2");
        enable_encoder!(encoders, "wrapped_avframe");
        enable_encoder!(encoders, "xbm");
        enable_encoder!(encoders, "xface");
        enable_encoder!(encoders, "xsub");
        enable_encoder!(encoders, "xwd");
        enable_encoder!(encoders, "y41p");
        enable_encoder!(encoders, "yuv4");
        enable_encoder!(encoders, "zlib");
        enable_encoder!(encoders, "zmbv");

        if !encoders.is_empty() {
            configure.arg(format!("--enable-encoder={}", encoders.join(",")));
        }
    }

    // configure filters
    if env::var("CARGO_FEATURE_DISABLE_FILTERS").is_ok() {
        configure.arg("--disable-filters");

        macro_rules! enable_filter {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_FILTER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut filters: Vec<&str> = vec![];
        enable_filter!(filters, "abench");
        enable_filter!(filters, "abitscope");
        enable_filter!(filters, "acompressor");
        enable_filter!(filters, "acontrast");
        enable_filter!(filters, "acopy");
        enable_filter!(filters, "acrossfade");
        enable_filter!(filters, "acrossover");
        enable_filter!(filters, "acrusher");
        enable_filter!(filters, "acue");
        enable_filter!(filters, "addroi");
        enable_filter!(filters, "adeclick");
        enable_filter!(filters, "adeclip");
        enable_filter!(filters, "adelay");
        enable_filter!(filters, "aderivative");
        enable_filter!(filters, "adrawgraph");
        enable_filter!(filters, "aecho");
        enable_filter!(filters, "aemphasis");
        enable_filter!(filters, "aeval");
        enable_filter!(filters, "aevalsrc");
        enable_filter!(filters, "afade");
        enable_filter!(filters, "afftdn");
        enable_filter!(filters, "afftfilt");
        enable_filter!(filters, "afifo");
        enable_filter!(filters, "afir");
        enable_filter!(filters, "afirsrc");
        enable_filter!(filters, "aformat");
        enable_filter!(filters, "agate");
        enable_filter!(filters, "agraphmonitor");
        enable_filter!(filters, "ahistogram");
        enable_filter!(filters, "aiir");
        enable_filter!(filters, "aintegral");
        enable_filter!(filters, "ainterleave");
        enable_filter!(filters, "alimiter");
        enable_filter!(filters, "allpass");
        enable_filter!(filters, "allrgb");
        enable_filter!(filters, "allyuv");
        enable_filter!(filters, "aloop");
        enable_filter!(filters, "alphaextract");
        enable_filter!(filters, "alphamerge");
        enable_filter!(filters, "amerge");
        enable_filter!(filters, "ametadata");
        enable_filter!(filters, "amix");
        enable_filter!(filters, "amovie");
        enable_filter!(filters, "amplify");
        enable_filter!(filters, "amultiply");
        enable_filter!(filters, "anequalizer");
        enable_filter!(filters, "anlmdn");
        enable_filter!(filters, "anlms");
        enable_filter!(filters, "anoisesrc");
        enable_filter!(filters, "anull");
        enable_filter!(filters, "anullsink");
        enable_filter!(filters, "anullsrc");
        enable_filter!(filters, "apad");
        enable_filter!(filters, "aperms");
        enable_filter!(filters, "aphasemeter");
        enable_filter!(filters, "aphaser");
        enable_filter!(filters, "apulsator");
        enable_filter!(filters, "arealtime");
        enable_filter!(filters, "aresample");
        enable_filter!(filters, "areverse");
        enable_filter!(filters, "arnndn");
        enable_filter!(filters, "aselect");
        enable_filter!(filters, "asendcmd");
        enable_filter!(filters, "asetnsamples");
        enable_filter!(filters, "asetpts");
        enable_filter!(filters, "asetrate");
        enable_filter!(filters, "asettb");
        enable_filter!(filters, "ashowinfo");
        enable_filter!(filters, "asidedata");
        enable_filter!(filters, "asoftclip");
        enable_filter!(filters, "asplit");
        enable_filter!(filters, "asr");
        enable_filter!(filters, "ass");
        enable_filter!(filters, "astats");
        enable_filter!(filters, "astreamselect");
        enable_filter!(filters, "asubboost");
        enable_filter!(filters, "atadenoise");
        enable_filter!(filters, "atempo");
        enable_filter!(filters, "atrim");
        enable_filter!(filters, "avectorscope");
        enable_filter!(filters, "avgblur");
        enable_filter!(filters, "avgblur_opencl");
        enable_filter!(filters, "avgblur_vulkan");
        enable_filter!(filters, "axcorrelate");
        enable_filter!(filters, "azmq");
        enable_filter!(filters, "bandpass");
        enable_filter!(filters, "bandreject");
        enable_filter!(filters, "bass");
        enable_filter!(filters, "bbox");
        enable_filter!(filters, "bench");
        enable_filter!(filters, "bilateral");
        enable_filter!(filters, "biquad");
        enable_filter!(filters, "bitplanenoise");
        enable_filter!(filters, "blackdetect");
        enable_filter!(filters, "blackframe");
        enable_filter!(filters, "blend");
        enable_filter!(filters, "bm3d");
        enable_filter!(filters, "boxblur");
        enable_filter!(filters, "boxblur_opencl");
        enable_filter!(filters, "bs2b");
        enable_filter!(filters, "bwdif");
        enable_filter!(filters, "cas");
        enable_filter!(filters, "cellauto");
        enable_filter!(filters, "channelmap");
        enable_filter!(filters, "channelsplit");
        enable_filter!(filters, "chorus");
        enable_filter!(filters, "chromaber_vulkan");
        enable_filter!(filters, "chromahold");
        enable_filter!(filters, "chromakey");
        enable_filter!(filters, "chromanr");
        enable_filter!(filters, "chromashift");
        enable_filter!(filters, "ciescope");
        enable_filter!(filters, "codecview");
        enable_filter!(filters, "color");
        enable_filter!(filters, "colorbalance");
        enable_filter!(filters, "colorchannelmixer");
        enable_filter!(filters, "colorhold");
        enable_filter!(filters, "colorkey");
        enable_filter!(filters, "colorkey_opencl");
        enable_filter!(filters, "colorlevels");
        enable_filter!(filters, "colormatrix");
        enable_filter!(filters, "colorspace");
        enable_filter!(filters, "compand");
        enable_filter!(filters, "compensationdelay");
        enable_filter!(filters, "concat");
        enable_filter!(filters, "convolution");
        enable_filter!(filters, "convolution_opencl");
        enable_filter!(filters, "convolve");
        enable_filter!(filters, "copy");
        enable_filter!(filters, "coreimage");
        enable_filter!(filters, "coreimagesrc");
        enable_filter!(filters, "cover_rect");
        enable_filter!(filters, "crop");
        enable_filter!(filters, "cropdetect");
        enable_filter!(filters, "crossfeed");
        enable_filter!(filters, "crystalizer");
        enable_filter!(filters, "cue");
        enable_filter!(filters, "curves");
        enable_filter!(filters, "datascope");
        enable_filter!(filters, "dblur");
        enable_filter!(filters, "dcshift");
        enable_filter!(filters, "dctdnoiz");
        enable_filter!(filters, "deband");
        enable_filter!(filters, "deblock");
        enable_filter!(filters, "decimate");
        enable_filter!(filters, "deconvolve");
        enable_filter!(filters, "dedot");
        enable_filter!(filters, "deesser");
        enable_filter!(filters, "deflate");
        enable_filter!(filters, "deflicker");
        enable_filter!(filters, "deinterlace_qsv");
        enable_filter!(filters, "deinterlace_vaapi");
        enable_filter!(filters, "dejudder");
        enable_filter!(filters, "delogo");
        enable_filter!(filters, "denoise_vaapi");
        enable_filter!(filters, "derain");
        enable_filter!(filters, "deshake");
        enable_filter!(filters, "deshake_opencl");
        enable_filter!(filters, "despill");
        enable_filter!(filters, "detelecine");
        enable_filter!(filters, "dilation");
        enable_filter!(filters, "dilation_opencl");
        enable_filter!(filters, "displace");
        enable_filter!(filters, "dnn_processing");
        enable_filter!(filters, "doubleweave");
        enable_filter!(filters, "drawbox");
        enable_filter!(filters, "drawgraph");
        enable_filter!(filters, "drawgrid");
        enable_filter!(filters, "drawtext");
        enable_filter!(filters, "drmeter");
        enable_filter!(filters, "dynaudnorm");
        enable_filter!(filters, "earwax");
        enable_filter!(filters, "ebur128");
        enable_filter!(filters, "edgedetect");
        enable_filter!(filters, "elbg");
        enable_filter!(filters, "entropy");
        enable_filter!(filters, "eq");
        enable_filter!(filters, "equalizer");
        enable_filter!(filters, "erosion");
        enable_filter!(filters, "erosion_opencl");
        enable_filter!(filters, "extractplanes");
        enable_filter!(filters, "extrastereo");
        enable_filter!(filters, "fade");
        enable_filter!(filters, "fftdnoiz");
        enable_filter!(filters, "fftfilt");
        enable_filter!(filters, "field");
        enable_filter!(filters, "fieldhint");
        enable_filter!(filters, "fieldmatch");
        enable_filter!(filters, "fieldorder");
        enable_filter!(filters, "fifo");
        enable_filter!(filters, "fillborders");
        enable_filter!(filters, "find_rect");
        enable_filter!(filters, "firequalizer");
        enable_filter!(filters, "flanger");
        enable_filter!(filters, "flite");
        enable_filter!(filters, "floodfill");
        enable_filter!(filters, "format");
        enable_filter!(filters, "fps");
        enable_filter!(filters, "framepack");
        enable_filter!(filters, "framerate");
        enable_filter!(filters, "framestep");
        enable_filter!(filters, "freezedetect");
        enable_filter!(filters, "freezeframes");
        enable_filter!(filters, "frei0r");
        enable_filter!(filters, "frei0r_src");
        enable_filter!(filters, "fspp");
        enable_filter!(filters, "gblur");
        enable_filter!(filters, "geq");
        enable_filter!(filters, "gradfun");
        enable_filter!(filters, "gradients");
        enable_filter!(filters, "graphmonitor");
        enable_filter!(filters, "greyedge");
        enable_filter!(filters, "haas");
        enable_filter!(filters, "haldclut");
        enable_filter!(filters, "haldclutsrc");
        enable_filter!(filters, "hdcd");
        enable_filter!(filters, "headphone");
        enable_filter!(filters, "hflip");
        enable_filter!(filters, "highpass");
        enable_filter!(filters, "highshelf");
        enable_filter!(filters, "hilbert");
        enable_filter!(filters, "histeq");
        enable_filter!(filters, "histogram");
        enable_filter!(filters, "hqdn3d");
        enable_filter!(filters, "hqx");
        enable_filter!(filters, "hstack");
        enable_filter!(filters, "hue");
        enable_filter!(filters, "hwdownload");
        enable_filter!(filters, "hwmap");
        enable_filter!(filters, "hwupload");
        enable_filter!(filters, "hwupload_cuda");
        enable_filter!(filters, "hysteresis");
        enable_filter!(filters, "idet");
        enable_filter!(filters, "il");
        enable_filter!(filters, "inflate");
        enable_filter!(filters, "interlace");
        enable_filter!(filters, "interleave");
        enable_filter!(filters, "join");
        enable_filter!(filters, "kerndeint");
        enable_filter!(filters, "ladspa");
        enable_filter!(filters, "lagfun");
        enable_filter!(filters, "lenscorrection");
        enable_filter!(filters, "lensfun");
        enable_filter!(filters, "libvmaf");
        enable_filter!(filters, "life");
        enable_filter!(filters, "limiter");
        enable_filter!(filters, "loop");
        enable_filter!(filters, "loudnorm");
        enable_filter!(filters, "lowpass");
        enable_filter!(filters, "lowshelf");
        enable_filter!(filters, "lumakey");
        enable_filter!(filters, "lut");
        enable_filter!(filters, "lut1d");
        enable_filter!(filters, "lut2");
        enable_filter!(filters, "lut3d");
        enable_filter!(filters, "lutrgb");
        enable_filter!(filters, "lutyuv");
        enable_filter!(filters, "lv2");
        enable_filter!(filters, "mandelbrot");
        enable_filter!(filters, "maskedclamp");
        enable_filter!(filters, "maskedmax");
        enable_filter!(filters, "maskedmerge");
        enable_filter!(filters, "maskedmin");
        enable_filter!(filters, "maskedthreshold");
        enable_filter!(filters, "maskfun");
        enable_filter!(filters, "mcdeint");
        enable_filter!(filters, "mcompand");
        enable_filter!(filters, "median");
        enable_filter!(filters, "mergeplanes");
        enable_filter!(filters, "mestimate");
        enable_filter!(filters, "metadata");
        enable_filter!(filters, "midequalizer");
        enable_filter!(filters, "minterpolate");
        enable_filter!(filters, "mix");
        enable_filter!(filters, "movie");
        enable_filter!(filters, "mpdecimate");
        enable_filter!(filters, "mptestsrc");
        enable_filter!(filters, "negate");
        enable_filter!(filters, "nlmeans");
        enable_filter!(filters, "nlmeans_opencl");
        enable_filter!(filters, "nnedi");
        enable_filter!(filters, "noformat");
        enable_filter!(filters, "noise");
        enable_filter!(filters, "normalize");
        enable_filter!(filters, "null");
        enable_filter!(filters, "nullsink");
        enable_filter!(filters, "nullsrc");
        enable_filter!(filters, "ocr");
        enable_filter!(filters, "ocv");
        enable_filter!(filters, "openclsrc");
        enable_filter!(filters, "oscilloscope");
        enable_filter!(filters, "overlay");
        enable_filter!(filters, "overlay_cuda");
        enable_filter!(filters, "overlay_opencl");
        enable_filter!(filters, "overlay_qsv");
        enable_filter!(filters, "overlay_vulkan");
        enable_filter!(filters, "owdenoise");
        enable_filter!(filters, "pad");
        enable_filter!(filters, "pad_opencl");
        enable_filter!(filters, "pal100bars");
        enable_filter!(filters, "pal75bars");
        enable_filter!(filters, "palettegen");
        enable_filter!(filters, "paletteuse");
        enable_filter!(filters, "pan");
        enable_filter!(filters, "perms");
        enable_filter!(filters, "perspective");
        enable_filter!(filters, "phase");
        enable_filter!(filters, "photosensitivity");
        enable_filter!(filters, "pixdesctest");
        enable_filter!(filters, "pixscope");
        enable_filter!(filters, "pp");
        enable_filter!(filters, "pp7");
        enable_filter!(filters, "premultiply");
        enable_filter!(filters, "prewitt");
        enable_filter!(filters, "prewitt_opencl");
        enable_filter!(filters, "procamp_vaapi");
        enable_filter!(filters, "program_opencl");
        enable_filter!(filters, "pseudocolor");
        enable_filter!(filters, "psnr");
        enable_filter!(filters, "pullup");
        enable_filter!(filters, "qp");
        enable_filter!(filters, "random");
        enable_filter!(filters, "readeia608");
        enable_filter!(filters, "readvitc");
        enable_filter!(filters, "realtime");
        enable_filter!(filters, "remap");
        enable_filter!(filters, "removegrain");
        enable_filter!(filters, "removelogo");
        enable_filter!(filters, "repeatfields");
        enable_filter!(filters, "replaygain");
        enable_filter!(filters, "resample");
        enable_filter!(filters, "reverse");
        enable_filter!(filters, "rgbashift");
        enable_filter!(filters, "rgbtestsrc");
        enable_filter!(filters, "roberts");
        enable_filter!(filters, "roberts_opencl");
        enable_filter!(filters, "rotate");
        enable_filter!(filters, "rubberband");
        enable_filter!(filters, "sab");
        enable_filter!(filters, "scale");
        enable_filter!(filters, "scale_cuda");
        enable_filter!(filters, "scale_npp");
        enable_filter!(filters, "scale_qsv");
        enable_filter!(filters, "scale_vaapi");
        enable_filter!(filters, "scale_vulkan");
        enable_filter!(filters, "scale2ref");
        enable_filter!(filters, "scdet");
        enable_filter!(filters, "scroll");
        enable_filter!(filters, "select");
        enable_filter!(filters, "selectivecolor");
        enable_filter!(filters, "sendcmd");
        enable_filter!(filters, "separatefields");
        enable_filter!(filters, "setdar");
        enable_filter!(filters, "setfield");
        enable_filter!(filters, "setparams");
        enable_filter!(filters, "setpts");
        enable_filter!(filters, "setrange");
        enable_filter!(filters, "setsar");
        enable_filter!(filters, "settb");
        enable_filter!(filters, "sharpness_vaapi");
        enable_filter!(filters, "showcqt");
        enable_filter!(filters, "showfreqs");
        enable_filter!(filters, "showinfo");
        enable_filter!(filters, "showpalette");
        enable_filter!(filters, "showspatial");
        enable_filter!(filters, "showspectrum");
        enable_filter!(filters, "showspectrumpic");
        enable_filter!(filters, "showvolume");
        enable_filter!(filters, "showwaves");
        enable_filter!(filters, "showwavespic");
        enable_filter!(filters, "shuffleframes");
        enable_filter!(filters, "shuffleplanes");
        enable_filter!(filters, "sidechaincompress");
        enable_filter!(filters, "sidechaingate");
        enable_filter!(filters, "sidedata");
        enable_filter!(filters, "sierpinski");
        enable_filter!(filters, "signalstats");
        enable_filter!(filters, "signature");
        enable_filter!(filters, "silencedetect");
        enable_filter!(filters, "silenceremove");
        enable_filter!(filters, "sinc");
        enable_filter!(filters, "sine");
        enable_filter!(filters, "smartblur");
        enable_filter!(filters, "smptebars");
        enable_filter!(filters, "smptehdbars");
        enable_filter!(filters, "sobel");
        enable_filter!(filters, "sobel_opencl");
        enable_filter!(filters, "sofalizer");
        enable_filter!(filters, "spectrumsynth");
        enable_filter!(filters, "split");
        enable_filter!(filters, "spp");
        enable_filter!(filters, "sr");
        enable_filter!(filters, "ssim");
        enable_filter!(filters, "stereo3d");
        enable_filter!(filters, "stereotools");
        enable_filter!(filters, "stereowiden");
        enable_filter!(filters, "streamselect");
        enable_filter!(filters, "subtitles");
        enable_filter!(filters, "super2xsai");
        enable_filter!(filters, "superequalizer");
        enable_filter!(filters, "surround");
        enable_filter!(filters, "swaprect");
        enable_filter!(filters, "swapuv");
        enable_filter!(filters, "tblend");
        enable_filter!(filters, "telecine");
        enable_filter!(filters, "testsrc");
        enable_filter!(filters, "testsrc2");
        enable_filter!(filters, "thistogram");
        enable_filter!(filters, "threshold");
        enable_filter!(filters, "thumbnail");
        enable_filter!(filters, "thumbnail_cuda");
        enable_filter!(filters, "tile");
        enable_filter!(filters, "tinterlace");
        enable_filter!(filters, "tlut2");
        enable_filter!(filters, "tmedian");
        enable_filter!(filters, "tmix");
        enable_filter!(filters, "tonemap");
        enable_filter!(filters, "tonemap_opencl");
        enable_filter!(filters, "tonemap_vaapi");
        enable_filter!(filters, "tpad");
        enable_filter!(filters, "transpose");
        enable_filter!(filters, "transpose_npp");
        enable_filter!(filters, "transpose_opencl");
        enable_filter!(filters, "transpose_vaapi");
        enable_filter!(filters, "treble");
        enable_filter!(filters, "tremolo");
        enable_filter!(filters, "trim");
        enable_filter!(filters, "unpremultiply");
        enable_filter!(filters, "unsharp");
        enable_filter!(filters, "unsharp_opencl");
        enable_filter!(filters, "untile");
        enable_filter!(filters, "uspp");
        enable_filter!(filters, "v360");
        enable_filter!(filters, "vaguedenoiser");
        enable_filter!(filters, "vectorscope");
        enable_filter!(filters, "vflip");
        enable_filter!(filters, "vfrdet");
        enable_filter!(filters, "vibrance");
        enable_filter!(filters, "vibrato");
        enable_filter!(filters, "vidstabdetect");
        enable_filter!(filters, "vidstabtransform");
        enable_filter!(filters, "vignette");
        enable_filter!(filters, "vmafmotion");
        enable_filter!(filters, "volume");
        enable_filter!(filters, "volumedetect");
        enable_filter!(filters, "vpp_qsv");
        enable_filter!(filters, "vstack");
        enable_filter!(filters, "w3fdif");
        enable_filter!(filters, "waveform");
        enable_filter!(filters, "weave");
        enable_filter!(filters, "xbr");
        enable_filter!(filters, "xfade");
        enable_filter!(filters, "xfade_opencl");
        enable_filter!(filters, "xmedian");
        enable_filter!(filters, "xstack");
        enable_filter!(filters, "yadif");
        enable_filter!(filters, "yadif_cuda");
        enable_filter!(filters, "yaepblur");
        enable_filter!(filters, "yuvtestsrc");
        enable_filter!(filters, "zmq");
        enable_filter!(filters, "zoompan");
        enable_filter!(filters, "zscale");

        if !filters.is_empty() {
            configure.arg(format!("--enable-filter={}", filters.join(",")));
        }
    }

    // configure hwaccels
    if env::var("CARGO_FEATURE_DISABLE_HWACCELS").is_ok() {
        configure.arg("--disable-hwaccels");

        macro_rules! enable_hwaccel {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_HWACCEL_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut hwaccels: Vec<&str> = vec![];
        enable_hwaccel!(hwaccels, "h263_vaapi");
        enable_hwaccel!(hwaccels, "h263_videotoolbox");
        enable_hwaccel!(hwaccels, "h264_d3d11va");
        enable_hwaccel!(hwaccels, "h264_d3d11va2");
        enable_hwaccel!(hwaccels, "h264_dxva2");
        enable_hwaccel!(hwaccels, "h264_nvdec");
        enable_hwaccel!(hwaccels, "h264_vaapi");
        enable_hwaccel!(hwaccels, "h264_vdpau");
        enable_hwaccel!(hwaccels, "h264_videotoolbox");
        enable_hwaccel!(hwaccels, "hevc_d3d11va");
        enable_hwaccel!(hwaccels, "hevc_d3d11va2");
        enable_hwaccel!(hwaccels, "hevc_dxva2");
        enable_hwaccel!(hwaccels, "hevc_nvdec");
        enable_hwaccel!(hwaccels, "hevc_vaapi");
        enable_hwaccel!(hwaccels, "hevc_vdpau");
        enable_hwaccel!(hwaccels, "hevc_videotoolbox");
        enable_hwaccel!(hwaccels, "mjpeg_nvdec");
        enable_hwaccel!(hwaccels, "mjpeg_vaapi");
        enable_hwaccel!(hwaccels, "mpeg1_nvdec");
        enable_hwaccel!(hwaccels, "mpeg1_vdpau");
        enable_hwaccel!(hwaccels, "mpeg1_videotoolbox");
        enable_hwaccel!(hwaccels, "mpeg1_xvmc");
        enable_hwaccel!(hwaccels, "mpeg2_d3d11va");
        enable_hwaccel!(hwaccels, "mpeg2_d3d11va2");
        enable_hwaccel!(hwaccels, "mpeg2_dxva2");
        enable_hwaccel!(hwaccels, "mpeg2_nvdec");
        enable_hwaccel!(hwaccels, "mpeg2_vaapi");
        enable_hwaccel!(hwaccels, "mpeg2_vdpau");
        enable_hwaccel!(hwaccels, "mpeg2_videotoolbox");
        enable_hwaccel!(hwaccels, "mpeg2_xvmc");
        enable_hwaccel!(hwaccels, "mpeg4_nvdec");
        enable_hwaccel!(hwaccels, "mpeg4_vaapi");
        enable_hwaccel!(hwaccels, "mpeg4_vdpau");
        enable_hwaccel!(hwaccels, "mpeg4_videotoolbox");
        enable_hwaccel!(hwaccels, "vc1_d3d11va");
        enable_hwaccel!(hwaccels, "vc1_d3d11va2");
        enable_hwaccel!(hwaccels, "vc1_dxva2");
        enable_hwaccel!(hwaccels, "vc1_nvdec");
        enable_hwaccel!(hwaccels, "vc1_vaapi");
        enable_hwaccel!(hwaccels, "vc1_vdpau");
        enable_hwaccel!(hwaccels, "vp8_nvdec");
        enable_hwaccel!(hwaccels, "vp8_vaapi");
        enable_hwaccel!(hwaccels, "vp9_d3d11va");
        enable_hwaccel!(hwaccels, "vp9_d3d11va2");
        enable_hwaccel!(hwaccels, "vp9_dxva2");
        enable_hwaccel!(hwaccels, "vp9_nvdec");
        enable_hwaccel!(hwaccels, "vp9_vaapi");
        enable_hwaccel!(hwaccels, "vp9_vdpau");
        enable_hwaccel!(hwaccels, "wmv3_d3d11va");
        enable_hwaccel!(hwaccels, "wmv3_d3d11va2");
        enable_hwaccel!(hwaccels, "wmv3_dxva2");
        enable_hwaccel!(hwaccels, "wmv3_nvdec");
        enable_hwaccel!(hwaccels, "wmv3_vaapi");
        enable_hwaccel!(hwaccels, "wmv3_vdpau");

        if !hwaccels.is_empty() {
            configure.arg(format!("--enable-hwaccel={}", hwaccels.join(",")));
        }
    }

    // configure indevs
    if env::var("CARGO_FEATURE_DISABLE_INDEVS").is_ok() {
        configure.arg("--disable-indevs");

        macro_rules! enable_indev {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_INDEV_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut indevs: Vec<&str> = vec![];
        enable_indev!(indevs, "alsa");
        enable_indev!(indevs, "android_camera");
        enable_indev!(indevs, "avfoundation");
        enable_indev!(indevs, "bktr");
        enable_indev!(indevs, "decklink");
        enable_indev!(indevs, "dshow");
        enable_indev!(indevs, "fbdev");
        enable_indev!(indevs, "gdigrab");
        enable_indev!(indevs, "iec61883");
        enable_indev!(indevs, "jack");
        enable_indev!(indevs, "kmsgrab");
        enable_indev!(indevs, "lavfi");
        enable_indev!(indevs, "libcdio");
        enable_indev!(indevs, "libdc1394");
        enable_indev!(indevs, "openal");
        enable_indev!(indevs, "oss");
        enable_indev!(indevs, "pulse");
        enable_indev!(indevs, "sndio");
        enable_indev!(indevs, "v4l2");
        enable_indev!(indevs, "vfwcap");
        enable_indev!(indevs, "xcbgrab");

        if !indevs.is_empty() {
            configure.arg(format!("--enable-indevs={}", indevs.join(",")));
        }
    }

    // configure muxers
    if env::var("CARGO_FEATURE_DISABLE_MUXERS").is_ok() {
        configure.arg("--disable-muxers");

        macro_rules! enable_muxer {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_MUXER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut muxers: Vec<&str> = vec![];
        enable_muxer!(muxers, "a64");
        enable_muxer!(muxers, "ac3");
        enable_muxer!(muxers, "adts");
        enable_muxer!(muxers, "adx");
        enable_muxer!(muxers, "aiff");
        enable_muxer!(muxers, "amr");
        enable_muxer!(muxers, "apm");
        enable_muxer!(muxers, "apng");
        enable_muxer!(muxers, "aptx");
        enable_muxer!(muxers, "aptx_hd");
        enable_muxer!(muxers, "asf");
        enable_muxer!(muxers, "asf_stream");
        enable_muxer!(muxers, "ass");
        enable_muxer!(muxers, "ast");
        enable_muxer!(muxers, "au");
        enable_muxer!(muxers, "avi");
        enable_muxer!(muxers, "avm2");
        enable_muxer!(muxers, "avs2");
        enable_muxer!(muxers, "bit");
        enable_muxer!(muxers, "caf");
        enable_muxer!(muxers, "cavsvideo");
        enable_muxer!(muxers, "chromaprint");
        enable_muxer!(muxers, "codec2");
        enable_muxer!(muxers, "codec2raw");
        enable_muxer!(muxers, "crc");
        enable_muxer!(muxers, "dash");
        enable_muxer!(muxers, "data");
        enable_muxer!(muxers, "daud");
        enable_muxer!(muxers, "dirac");
        enable_muxer!(muxers, "dnxhd");
        enable_muxer!(muxers, "dts");
        enable_muxer!(muxers, "dv");
        enable_muxer!(muxers, "eac3");
        enable_muxer!(muxers, "f4v");
        enable_muxer!(muxers, "ffmetadata");
        enable_muxer!(muxers, "fifo");
        enable_muxer!(muxers, "fifo_test");
        enable_muxer!(muxers, "filmstrip");
        enable_muxer!(muxers, "fits");
        enable_muxer!(muxers, "flac");
        enable_muxer!(muxers, "flv");
        enable_muxer!(muxers, "framecrc");
        enable_muxer!(muxers, "framehash");
        enable_muxer!(muxers, "framemd5");
        enable_muxer!(muxers, "g722");
        enable_muxer!(muxers, "g723_1");
        enable_muxer!(muxers, "g726");
        enable_muxer!(muxers, "g726le");
        enable_muxer!(muxers, "gif");
        enable_muxer!(muxers, "gsm");
        enable_muxer!(muxers, "gxf");
        enable_muxer!(muxers, "h261");
        enable_muxer!(muxers, "h263");
        enable_muxer!(muxers, "h264");
        enable_muxer!(muxers, "hash");
        enable_muxer!(muxers, "hds");
        enable_muxer!(muxers, "hevc");
        enable_muxer!(muxers, "hls");
        enable_muxer!(muxers, "ico");
        enable_muxer!(muxers, "ilbc");
        enable_muxer!(muxers, "image2");
        enable_muxer!(muxers, "image2pipe");
        enable_muxer!(muxers, "ipod");
        enable_muxer!(muxers, "ircam");
        enable_muxer!(muxers, "ismv");
        enable_muxer!(muxers, "ivf");
        enable_muxer!(muxers, "jacosub");
        enable_muxer!(muxers, "kvag");
        enable_muxer!(muxers, "latm");
        enable_muxer!(muxers, "lrc");
        enable_muxer!(muxers, "m4v");
        enable_muxer!(muxers, "matroska");
        enable_muxer!(muxers, "matroska_audio");
        enable_muxer!(muxers, "md5");
        enable_muxer!(muxers, "microdvd");
        enable_muxer!(muxers, "mjpeg");
        enable_muxer!(muxers, "mkvtimestamp_v2");
        enable_muxer!(muxers, "mlp");
        enable_muxer!(muxers, "mmf");
        enable_muxer!(muxers, "mov");
        enable_muxer!(muxers, "mp2");
        enable_muxer!(muxers, "mp3");
        enable_muxer!(muxers, "mp4");
        enable_muxer!(muxers, "mpeg1system");
        enable_muxer!(muxers, "mpeg1vcd");
        enable_muxer!(muxers, "mpeg1video");
        enable_muxer!(muxers, "mpeg2dvd");
        enable_muxer!(muxers, "mpeg2svcd");
        enable_muxer!(muxers, "mpeg2video");
        enable_muxer!(muxers, "mpeg2vob");
        enable_muxer!(muxers, "mpegts");
        enable_muxer!(muxers, "mpjpeg");
        enable_muxer!(muxers, "mxf");
        enable_muxer!(muxers, "mxf_d10");
        enable_muxer!(muxers, "mxf_opatom");
        enable_muxer!(muxers, "null");
        enable_muxer!(muxers, "nut");
        enable_muxer!(muxers, "oga");
        enable_muxer!(muxers, "ogg");
        enable_muxer!(muxers, "ogv");
        enable_muxer!(muxers, "oma");
        enable_muxer!(muxers, "opus");
        enable_muxer!(muxers, "pcm_alaw");
        enable_muxer!(muxers, "pcm_f32be");
        enable_muxer!(muxers, "pcm_f32le");
        enable_muxer!(muxers, "pcm_f64be");
        enable_muxer!(muxers, "pcm_f64le");
        enable_muxer!(muxers, "pcm_mulaw");
        enable_muxer!(muxers, "pcm_s16be");
        enable_muxer!(muxers, "pcm_s16le");
        enable_muxer!(muxers, "pcm_s24be");
        enable_muxer!(muxers, "pcm_s24le");
        enable_muxer!(muxers, "pcm_s32be");
        enable_muxer!(muxers, "pcm_s32le");
        enable_muxer!(muxers, "pcm_s8");
        enable_muxer!(muxers, "pcm_u16be");
        enable_muxer!(muxers, "pcm_u16le");
        enable_muxer!(muxers, "pcm_u24be");
        enable_muxer!(muxers, "pcm_u24le");
        enable_muxer!(muxers, "pcm_u32be");
        enable_muxer!(muxers, "pcm_u32le");
        enable_muxer!(muxers, "pcm_u8");
        enable_muxer!(muxers, "pcm_vidc");
        enable_muxer!(muxers, "psp");
        enable_muxer!(muxers, "rawvideo");
        enable_muxer!(muxers, "rm");
        enable_muxer!(muxers, "roq");
        enable_muxer!(muxers, "rso");
        enable_muxer!(muxers, "rtp");
        enable_muxer!(muxers, "rtp_mpegts");
        enable_muxer!(muxers, "rtsp");
        enable_muxer!(muxers, "sap");
        enable_muxer!(muxers, "sbc");
        enable_muxer!(muxers, "scc");
        enable_muxer!(muxers, "segafilm");
        enable_muxer!(muxers, "segment");
        enable_muxer!(muxers, "singlejpeg");
        enable_muxer!(muxers, "smjpeg");
        enable_muxer!(muxers, "smoothstreaming");
        enable_muxer!(muxers, "sox");
        enable_muxer!(muxers, "spdif");
        enable_muxer!(muxers, "spx");
        enable_muxer!(muxers, "srt");
        enable_muxer!(muxers, "stream_segment");
        enable_muxer!(muxers, "streamhash");
        enable_muxer!(muxers, "sup");
        enable_muxer!(muxers, "swf");
        enable_muxer!(muxers, "tee");
        enable_muxer!(muxers, "tg2");
        enable_muxer!(muxers, "tgp");
        enable_muxer!(muxers, "truehd");
        enable_muxer!(muxers, "tta");
        enable_muxer!(muxers, "uncodedframecrc");
        enable_muxer!(muxers, "vc1");
        enable_muxer!(muxers, "vc1t");
        enable_muxer!(muxers, "voc");
        enable_muxer!(muxers, "w64");
        enable_muxer!(muxers, "wav");
        enable_muxer!(muxers, "webm");
        enable_muxer!(muxers, "webm_chunk");
        enable_muxer!(muxers, "webm_dash_manifest");
        enable_muxer!(muxers, "webp");
        enable_muxer!(muxers, "webvtt");
        enable_muxer!(muxers, "wtv");
        enable_muxer!(muxers, "wv");
        enable_muxer!(muxers, "yuv4mpegpipe");

        if !muxers.is_empty() {
            configure.arg(format!("--enable-muxer={}", muxers.join(",")));
        }
    }

    // configure outdevs
    if env::var("CARGO_FEATURE_DISABLE_OUTDEVS").is_ok() {
        configure.arg("--disable-outdevs");

        macro_rules! enable_outdev {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_OUTDEV_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut outdevs: Vec<&str> = vec![];
        enable_outdev!(outdevs, "alsa");
        enable_outdev!(outdevs, "audiotoolbox");
        enable_outdev!(outdevs, "caca");
        enable_outdev!(outdevs, "decklink");
        enable_outdev!(outdevs, "fbdev");
        enable_outdev!(outdevs, "opengl");
        enable_outdev!(outdevs, "oss");
        enable_outdev!(outdevs, "pulse");
        enable_outdev!(outdevs, "sdl2");
        enable_outdev!(outdevs, "sndio");
        enable_outdev!(outdevs, "v4l2");
        enable_outdev!(outdevs, "xv");
        if !outdevs.is_empty() {
            configure.arg(format!("--enable-outdev={}", outdevs.join(",")));
        }
    }

    // configure parsers
    if env::var("CARGO_FEATURE_DISABLE_PARSERS").is_ok() {
        configure.arg("--disable-parsers");

        macro_rules! enable_parser {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_PARSER_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut parsers: Vec<&str> = vec![];
        enable_parser!(parsers, "aac");
        enable_parser!(parsers, "aac_latm");
        enable_parser!(parsers, "ac3");
        enable_parser!(parsers, "adx");
        enable_parser!(parsers, "av1");
        enable_parser!(parsers, "avs2");
        enable_parser!(parsers, "bmp");
        enable_parser!(parsers, "cavsvideo");
        enable_parser!(parsers, "cook");
        enable_parser!(parsers, "dca");
        enable_parser!(parsers, "dirac");
        enable_parser!(parsers, "dnxhd");
        enable_parser!(parsers, "dpx");
        enable_parser!(parsers, "dvaudio");
        enable_parser!(parsers, "dvbsub");
        enable_parser!(parsers, "dvd_nav");
        enable_parser!(parsers, "dvdsub");
        enable_parser!(parsers, "flac");
        enable_parser!(parsers, "g723_1");
        enable_parser!(parsers, "g729");
        enable_parser!(parsers, "gif");
        enable_parser!(parsers, "gsm");
        enable_parser!(parsers, "h261");
        enable_parser!(parsers, "h263");
        enable_parser!(parsers, "h264");
        enable_parser!(parsers, "hevc");
        enable_parser!(parsers, "jpeg2000");
        enable_parser!(parsers, "mjpeg");
        enable_parser!(parsers, "mlp");
        enable_parser!(parsers, "mpeg4video");
        enable_parser!(parsers, "mpegaudio");
        enable_parser!(parsers, "mpegvideo");
        enable_parser!(parsers, "opus");
        enable_parser!(parsers, "png");
        enable_parser!(parsers, "pnm");
        enable_parser!(parsers, "rv30");
        enable_parser!(parsers, "rv40");
        enable_parser!(parsers, "sbc");
        enable_parser!(parsers, "sipr");
        enable_parser!(parsers, "tak");
        enable_parser!(parsers, "vc1");
        enable_parser!(parsers, "vorbis");
        enable_parser!(parsers, "vp3");
        enable_parser!(parsers, "vp8");
        enable_parser!(parsers, "vp9");
        enable_parser!(parsers, "webp");
        enable_parser!(parsers, "xma");

        if !parsers.is_empty() {
            configure.arg(format!("--enable-parser={}", parsers.join(",")));
        }
    }

    // configure protocols
    if env::var("CARGO_FEATURE_DISABLE_PROTOCOLS").is_ok() {
        configure.arg("--disable-protocols");

        macro_rules! enable_protocol {
            ($list:expr, $name:expr) => {
                let feat = $name.to_uppercase();
                if env::var(format!("CARGO_FEATURE_PROTOCOL_{}", feat)).is_ok() {
                    $list.push($name);
                }
            };
        }

        let mut protocols: Vec<&str> = vec![];
        enable_protocol!(protocols, "async");
        enable_protocol!(protocols, "bluray");
        enable_protocol!(protocols, "cache");
        enable_protocol!(protocols, "concat");
        enable_protocol!(protocols, "crypto");
        enable_protocol!(protocols, "data");
        enable_protocol!(protocols, "ffrtmpcrypt");
        enable_protocol!(protocols, "ffrtmphttp");
        enable_protocol!(protocols, "file");
        enable_protocol!(protocols, "ftp");
        enable_protocol!(protocols, "gopher");
        enable_protocol!(protocols, "hls");
        enable_protocol!(protocols, "http");
        enable_protocol!(protocols, "httpproxy");
        enable_protocol!(protocols, "https");
        enable_protocol!(protocols, "icecast");
        enable_protocol!(protocols, "libamqp");
        enable_protocol!(protocols, "librtmp");
        enable_protocol!(protocols, "librtmpe");
        enable_protocol!(protocols, "librtmps");
        enable_protocol!(protocols, "librtmpt");
        enable_protocol!(protocols, "librtmpte");
        enable_protocol!(protocols, "libsmbclient");
        enable_protocol!(protocols, "libsrt");
        enable_protocol!(protocols, "libssh");
        enable_protocol!(protocols, "libzmq");
        enable_protocol!(protocols, "md5");
        enable_protocol!(protocols, "mmsh");
        enable_protocol!(protocols, "mmst");
        enable_protocol!(protocols, "pipe");
        enable_protocol!(protocols, "prompeg");
        enable_protocol!(protocols, "rtmp");
        enable_protocol!(protocols, "rtmpe");
        enable_protocol!(protocols, "rtmps");
        enable_protocol!(protocols, "rtmpt");
        enable_protocol!(protocols, "rtmpte");
        enable_protocol!(protocols, "rtmpts");
        enable_protocol!(protocols, "rtp");
        enable_protocol!(protocols, "sctp");
        enable_protocol!(protocols, "srtp");
        enable_protocol!(protocols, "subfile");
        enable_protocol!(protocols, "tcp");
        enable_protocol!(protocols, "tee");
        enable_protocol!(protocols, "tls");
        enable_protocol!(protocols, "udp");
        enable_protocol!(protocols, "udplite");
        enable_protocol!(protocols, "unix");

        if !protocols.is_empty() {
            configure.arg(format!("--enable-protocol={}", protocols.join(",")));
        }
    }

    // run ./configure
    let output = configure
        .output()
        .unwrap_or_else(|_| panic!("{:?} failed", configure));
    if !output.status.success() {
        println!("configure: {}", String::from_utf8_lossy(&output.stdout));

        return Err(io::Error::new(
            io::ErrorKind::Other,
            format!(
                "configure failed {}",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    // run make
    if !Command::new("make")
        .arg("-j")
        .arg(num_cpus::get().to_string())
        .current_dir(&source())
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make failed"));
    }

    // run make install
    if !Command::new("make")
        .current_dir(&source())
        .arg("install")
        .status()?
        .success()
    {
        return Err(io::Error::new(io::ErrorKind::Other, "make install failed"));
    }

    Ok(())
}

fn check_features(
    include_paths: Vec<PathBuf>,
    infos: &[(&'static str, Option<&'static str>, &'static str)],
) {
    let mut includes_code = String::new();
    let mut main_code = String::new();

    for &(header, feature, var) in infos {
        if let Some(feature) = feature {
            if env::var(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_err() {
                continue;
            }
        }

        let include = format!("#include <{}>", header);
        if includes_code.find(&include).is_none() {
            includes_code.push_str(&include);
            includes_code.push_str(&"\n");
        }
        includes_code.push_str(&format!(
            r#"
            #ifndef {var}
            #define {var} 0
            #define {var}_is_defined 0
            #else
            #define {var}_is_defined 1
            #endif
        "#,
            var = var
        ));

        main_code.push_str(&format!(
            r#"printf("[{var}]%d%d\n", {var}, {var}_is_defined);"#,
            var = var
        ));
    }

    let version_check_info = [("avcodec", 56, 60, 0, 80)];
    for &(lib, begin_version_major, end_version_major, begin_version_minor, end_version_minor) in
        version_check_info.iter()
    {
        for version_major in begin_version_major..end_version_major {
            for version_minor in begin_version_minor..end_version_minor {
                main_code.push_str(&format!(
                    r#"printf("[{lib}_version_greater_than_{version_major}_{version_minor}]%d\n", LIB{lib_uppercase}_VERSION_MAJOR > {version_major} || (LIB{lib_uppercase}_VERSION_MAJOR == {version_major} && LIB{lib_uppercase}_VERSION_MINOR > {version_minor}));"#,
                    lib = lib,
                    lib_uppercase = lib.to_uppercase(),
                    version_major = version_major,
                    version_minor = version_minor
                ));
            }
        }
    }

    let out_dir = output();

    write!(
        File::create(out_dir.join("check.c")).expect("Failed to create file"),
        r#"
            #include <stdio.h>
            {includes_code}

            int main()
            {{
                {main_code}
                return 0;
            }}
           "#,
        includes_code = includes_code,
        main_code = main_code
    )
    .expect("Write failed");

    let executable = out_dir.join(if cfg!(windows) { "check.exe" } else { "check" });
    let mut compiler = cc::Build::new()
        .target(&env::var("HOST").unwrap())
        .get_compiler()
        .to_command();

    for dir in include_paths {
        compiler.arg("-I");
        compiler.arg(dir.to_string_lossy().into_owned());
    }
    if !compiler
        .current_dir(&out_dir)
        .arg("-o")
        .arg(&executable)
        .arg("check.c")
        .status()
        .expect("Command failed")
        .success()
    {
        panic!("Compile failed");
    }

    let stdout_raw = Command::new(out_dir.join(&executable))
        .current_dir(&out_dir)
        .output()
        .expect("Check failed")
        .stdout;
    let stdout = str::from_utf8(stdout_raw.as_slice()).unwrap();

    println!("stdout={}", stdout);

    for &(_, feature, var) in infos {
        if let Some(feature) = feature {
            if env::var(format!("CARGO_FEATURE_{}", feature.to_uppercase())).is_err() {
                continue;
            }
        }

        let var_str = format!("[{var}]", var = var);
        let pos = stdout.find(&var_str).expect("Variable not found in output") + var_str.len();
        if &stdout[pos..pos + 1] == "1" {
            println!(r#"cargo:rustc-cfg=feature="{}""#, var.to_lowercase());
            println!(r#"cargo:{}=true"#, var.to_lowercase());
        }

        // Also find out if defined or not (useful for cases where only the definition of a macro
        // can be used as distinction)
        if &stdout[pos + 1..pos + 2] == "1" {
            println!(
                r#"cargo:rustc-cfg=feature="{}_is_defined""#,
                var.to_lowercase()
            );
            println!(r#"cargo:{}_is_defined=true"#, var.to_lowercase());
        }
    }

    for &(lib, begin_version_major, end_version_major, begin_version_minor, end_version_minor) in
        version_check_info.iter()
    {
        for version_major in begin_version_major..end_version_major {
            for version_minor in begin_version_minor..end_version_minor {
                let search_str = format!(
                    "[{lib}_version_greater_than_{version_major}_{version_minor}]",
                    version_major = version_major,
                    version_minor = version_minor,
                    lib = lib
                );
                let pos = stdout
                    .find(&search_str)
                    .expect("Variable not found in output")
                    + search_str.len();

                if &stdout[pos..pos + 1] == "1" {
                    println!(
                        r#"cargo:rustc-cfg=feature="{}""#,
                        &search_str[1..(search_str.len() - 1)]
                    );
                }
            }
        }
    }
}

fn search_include(include_paths: &[PathBuf], header: &str) -> String {
    for dir in include_paths {
        let include = dir.join(header);
        if fs::metadata(&include).is_ok() {
            return include.as_path().to_str().unwrap().to_string();
        }
    }
    format!("/usr/include/{}", header)
}

fn search_include_optional(include_paths: &[PathBuf], header: &str) -> Option<String> {
    for dir in include_paths {
        let include = dir.join(header);
        if fs::metadata(&include).is_ok() {
            return Some(include.as_path().to_str().unwrap().to_string());
        }
    }
    None
}

fn link_to_libraries(statik: bool) {
    let ffmpeg_ty = if statik { "static" } else { "dylib" };
    for lib in LIBRARIES {
        let feat_is_enabled = lib.feature_name().and_then(|f| env::var(&f).ok()).is_some();
        if !lib.is_feature || feat_is_enabled {
            println!("cargo:rustc-link-lib={}={}", ffmpeg_ty, lib.name);
        }
    }
    if env::var("CARGO_FEATURE_BUILD_ZLIB").is_ok() && cfg!(target_os = "linux") {
        println!("cargo:rustc-link-lib=z");
    }
}

fn link_libs_for_module(module: &str) {
    let config_mak = source().join("ffbuild/config.mak");
    let file = File::open(config_mak).unwrap();
    let reader = BufReader::new(file);
    for line in reader.lines().map(|line| line.unwrap()) {
        if !line.starts_with("EXTRALIBS-") {
            continue;
        }
        if !line.contains(module) {
            continue;
        }
        let linker_args = line.split('=').last().unwrap().split(' ');
        let include_libs = linker_args
            .filter(|v| v.starts_with("-l"))
            .map(|flag| &flag[2..]);
        for lib in include_libs {
            println!("cargo:rustc-link-lib={}", lib);
        }
    }
}

fn main() {
    let statik = env::var("CARGO_FEATURE_STATIC").is_ok();

    let include_paths: Vec<PathBuf> = if env::var("CARGO_FEATURE_BUILD").is_ok() {
        println!(
            "cargo:rustc-link-search=native={}",
            search().join("lib").to_string_lossy()
        );
        link_to_libraries(statik);
        if fs::metadata(&search().join("lib").join("libavutil.a")).is_err() {
            fs::create_dir_all(&output()).expect("failed to create build directory");
            fetch().unwrap();
            build().unwrap();
        }

        // Check additional required libraries.
        {
            let config_mak = source().join("ffbuild/config.mak");
            let file = File::open(config_mak).unwrap();
            let reader = BufReader::new(file);
            let extra_libs = reader
                .lines()
                .find(|ref line| line.as_ref().unwrap().starts_with("EXTRALIBS"))
                .map(|line| line.unwrap())
                .unwrap();

            let linker_args = extra_libs.split('=').last().unwrap().split(' ');
            let include_libs = linker_args
                .filter(|v| v.starts_with("-l"))
                .map(|flag| &flag[2..]);

            for lib in include_libs {
                println!("cargo:rustc-link-lib={}", lib);
            }
        }

        // Check per-module required libraries.
        {
            let libs = vec![
                ("avcodec", "AVCODEC"),
                ("avdevice", "AVDEVICE"),
                ("avfilter", "AVFILTER"),
                ("avformat", "AVFORMAT"),
                ("avresample", "AVRESAMPLE"),
                ("avutil", "AVUTIL"),
                ("postproc", "POSTPROC"),
                ("swresample", "SWRESAMPLE"),
                ("swscale", "SWSCALE"),
            ];

            for (lib_name, env_variable_name) in libs.iter() {
                if env::var(format!("CARGO_FEATURE_{}", env_variable_name)).is_ok() {
                    link_libs_for_module(lib_name);
                }
            }
        }

        vec![search().join("include")]
    }
    // Use prebuilt library
    else if let Ok(ffmpeg_dir) = env::var("FFMPEG_DIR") {
        let ffmpeg_dir = PathBuf::from(ffmpeg_dir);
        println!(
            "cargo:rustc-link-search=native={}",
            ffmpeg_dir.join("lib").to_string_lossy()
        );
        link_to_libraries(statik);
        vec![ffmpeg_dir.join("include")]
    }
    // Fallback to pkg-config
    else {
        let mut all_paths: Vec<PathBuf> = vec![];
        let paths = pkg_config::Config::new()
            .statik(statik)
            .probe("libavutil")
            .unwrap()
            .include_paths;
        all_paths.extend(paths);

        let libs = vec![
            ("libavformat", "AVFORMAT"),
            ("libavfilter", "AVFILTER"),
            ("libavdevice", "AVDEVICE"),
            ("libavresample", "AVRESAMPLE"),
            ("libswscale", "SWSCALE"),
            ("libswresample", "SWRESAMPLE"),
        ];

        for (lib_name, env_variable_name) in libs.iter() {
            if env::var(format!("CARGO_FEATURE_{}", env_variable_name)).is_ok() {
                let paths = pkg_config::Config::new()
                    .statik(statik)
                    .probe(lib_name)
                    .unwrap()
                    .include_paths;
                all_paths.extend(paths);
            }
        }

        let paths = pkg_config::Config::new()
            .statik(statik)
            .probe("libavcodec")
            .unwrap()
            .include_paths;
        all_paths.extend(paths);

        all_paths
    };

    if statik && cfg!(target_os = "macos") {
        let frameworks = vec![
            "AppKit",
            "AudioToolbox",
            "AVFoundation",
            "CoreFoundation",
            "CoreGraphics",
            "CoreMedia",
            "CoreServices",
            "CoreVideo",
            "Foundation",
            "OpenCL",
            "OpenGL",
            "QTKit",
            "QuartzCore",
            "Security",
            "VideoDecodeAcceleration",
            "VideoToolbox",
        ];
        for f in frameworks {
            println!("cargo:rustc-link-lib=framework={}", f);
        }
    }

    check_features(
        include_paths.clone(),
        &[
            ("libavutil/avutil.h", None, "FF_API_OLD_AVOPTIONS"),
            ("libavutil/avutil.h", None, "FF_API_PIX_FMT"),
            ("libavutil/avutil.h", None, "FF_API_CONTEXT_SIZE"),
            ("libavutil/avutil.h", None, "FF_API_PIX_FMT_DESC"),
            ("libavutil/avutil.h", None, "FF_API_AV_REVERSE"),
            ("libavutil/avutil.h", None, "FF_API_AUDIOCONVERT"),
            ("libavutil/avutil.h", None, "FF_API_CPU_FLAG_MMX2"),
            ("libavutil/avutil.h", None, "FF_API_LLS_PRIVATE"),
            ("libavutil/avutil.h", None, "FF_API_AVFRAME_LAVC"),
            ("libavutil/avutil.h", None, "FF_API_VDPAU"),
            (
                "libavutil/avutil.h",
                None,
                "FF_API_GET_CHANNEL_LAYOUT_COMPAT",
            ),
            ("libavutil/avutil.h", None, "FF_API_XVMC"),
            ("libavutil/avutil.h", None, "FF_API_OPT_TYPE_METADATA"),
            ("libavutil/avutil.h", None, "FF_API_DLOG"),
            ("libavutil/avutil.h", None, "FF_API_HMAC"),
            ("libavutil/avutil.h", None, "FF_API_VAAPI"),
            ("libavutil/avutil.h", None, "FF_API_PKT_PTS"),
            ("libavutil/avutil.h", None, "FF_API_ERROR_FRAME"),
            ("libavutil/avutil.h", None, "FF_API_FRAME_QP"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_VIMA_DECODER",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_REQUEST_CHANNELS",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_DECODE_AUDIO",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_ENCODE_AUDIO",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_ENCODE_VIDEO",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_ID"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AUDIO_CONVERT",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AVCODEC_RESAMPLE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DEINTERLACE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DESTRUCT_PACKET",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_GET_BUFFER"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_MISSING_SAMPLE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_LOWRES"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CAP_VDPAU"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_BUFS_VDPAU"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VOXWARE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_SET_DIMENSIONS",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_DEBUG_MV"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AC_VLC"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_OLD_MSMPEG4",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_ASPECT_EXTENDED",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_THREAD_OPAQUE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_PKT"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_ALPHA"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_XVMC"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ERROR_RATE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_QSCALE_TYPE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MB_TYPE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_MAX_BFRAMES",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_NEG_LINESIZES",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_EMU_EDGE"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_SH4"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_ARCH_SPARC"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_UNUSED_MEMBERS",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_IDCT_XVIDMMX",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_INPUT_PRESERVED",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_NORMALIZE_AQP",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_GMC"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MV0"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODEC_NAME"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AFD"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VISMV"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_DV_FRAME_PROFILE",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AUDIOENC_DELAY",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_VAAPI_CONTEXT",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_AVCTX_TIMEBASE",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MPV_OPT"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_STREAM_CODEC_TAG",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_QUANT_BIAS"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_RC_STRATEGY",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_CODED_FRAME",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_MOTION_EST"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_WITHOUT_PREFIX",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_CONVERGENCE_DURATION",
            ),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_PRIVATE_OPT",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_CODER_TYPE"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_RTP_CALLBACK",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_STAT_BITS"),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_VBV_DELAY"),
            (
                "libavcodec/avcodec.h",
                Some("avcodec"),
                "FF_API_SIDEDATA_ONLY_PKT",
            ),
            ("libavcodec/avcodec.h", Some("avcodec"), "FF_API_AVPICTURE"),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_BITEXACT",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_FRAC",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_URL_FEOF",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_PROBESIZE_32",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_LAVF_AVCTX",
            ),
            (
                "libavformat/avformat.h",
                Some("avformat"),
                "FF_API_OLD_OPEN_CALLBACKS",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_AVFILTERPAD_PUBLIC",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_FOO_COUNT",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_OPTS",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_OPTS_ERROR",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_AVFILTER_OPEN",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_FILTER_REGISTER",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_OLD_GRAPH_PARSE",
            ),
            (
                "libavfilter/avfilter.h",
                Some("avfilter"),
                "FF_API_NOCONST_GET_NAME",
            ),
            (
                "libavresample/avresample.h",
                Some("avresample"),
                "FF_API_RESAMPLE_CLOSE_OPEN",
            ),
            (
                "libswscale/swscale.h",
                Some("swscale"),
                "FF_API_SWS_CPU_CAPS",
            ),
            ("libswscale/swscale.h", Some("swscale"), "FF_API_ARCH_BFIN"),
        ],
    );

    let clang_includes = include_paths
        .iter()
        .map(|include| format!("-I{}", include.to_string_lossy()));

    // The bindgen::Builder is the main entry point
    // to bindgen, and lets you build up options for
    // the resulting bindings.
    let mut builder = bindgen::Builder::default()
        .clang_args(clang_includes)
        .ctypes_prefix("libc")
        // https://github.com/servo/rust-bindgen/issues/550
        .blacklist_type("max_align_t")
        .rustified_enum("*")
        .prepend_enum_name(false)
        .derive_eq(true)
        .size_t_is_usize(true)
        .parse_callbacks(Box::new(Callbacks));

    // The input headers we would like to generate
    // bindings for.
    if env::var("CARGO_FEATURE_AVCODEC").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavcodec/avcodec.h"))
            .header(search_include(&include_paths, "libavcodec/dv_profile.h"))
            .header(search_include(&include_paths, "libavcodec/avfft.h"))
            .header(search_include(&include_paths, "libavcodec/vaapi.h"))
            .header(search_include(&include_paths, "libavcodec/vorbis_parser.h"));
    }

    if env::var("CARGO_FEATURE_AVDEVICE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libavdevice/avdevice.h"));
    }

    if env::var("CARGO_FEATURE_AVFILTER").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavfilter/buffersink.h"))
            .header(search_include(&include_paths, "libavfilter/buffersrc.h"))
            .header(search_include(&include_paths, "libavfilter/avfilter.h"));
    }

    if env::var("CARGO_FEATURE_AVFORMAT").is_ok() {
        builder = builder
            .header(search_include(&include_paths, "libavformat/avformat.h"))
            .header(search_include(&include_paths, "libavformat/avio.h"));
    }

    if env::var("CARGO_FEATURE_AVRESAMPLE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libavresample/avresample.h"));
    }

    builder = builder
        .header(search_include(&include_paths, "libavutil/adler32.h"))
        .header(search_include(&include_paths, "libavutil/aes.h"))
        .header(search_include(&include_paths, "libavutil/audio_fifo.h"))
        .header(search_include(&include_paths, "libavutil/base64.h"))
        .header(search_include(&include_paths, "libavutil/blowfish.h"))
        .header(search_include(&include_paths, "libavutil/bprint.h"))
        .header(search_include(&include_paths, "libavutil/buffer.h"))
        .header(search_include(&include_paths, "libavutil/camellia.h"))
        .header(search_include(&include_paths, "libavutil/cast5.h"))
        .header(search_include(&include_paths, "libavutil/channel_layout.h"))
        .header(search_include(&include_paths, "libavutil/cpu.h"))
        .header(search_include(&include_paths, "libavutil/crc.h"))
        .header(search_include(&include_paths, "libavutil/dict.h"))
        .header(search_include(&include_paths, "libavutil/display.h"))
        .header(search_include(&include_paths, "libavutil/downmix_info.h"))
        .header(search_include(&include_paths, "libavutil/error.h"))
        .header(search_include(&include_paths, "libavutil/eval.h"))
        .header(search_include(&include_paths, "libavutil/fifo.h"))
        .header(search_include(&include_paths, "libavutil/file.h"))
        .header(search_include(&include_paths, "libavutil/frame.h"))
        .header(search_include(&include_paths, "libavutil/hash.h"))
        .header(search_include(&include_paths, "libavutil/hmac.h"))
        .header(search_include(&include_paths, "libavutil/imgutils.h"))
        .header(search_include(&include_paths, "libavutil/lfg.h"))
        .header(search_include(&include_paths, "libavutil/log.h"))
        .header(search_include(&include_paths, "libavutil/macros.h"))
        .header(search_include(&include_paths, "libavutil/mathematics.h"))
        .header(search_include(&include_paths, "libavutil/md5.h"))
        .header(search_include(&include_paths, "libavutil/mem.h"))
        .header(search_include(&include_paths, "libavutil/motion_vector.h"))
        .header(search_include(&include_paths, "libavutil/murmur3.h"))
        .header(search_include(&include_paths, "libavutil/opt.h"))
        .header(search_include(&include_paths, "libavutil/parseutils.h"))
        .header(search_include(&include_paths, "libavutil/pixdesc.h"))
        .header(search_include(&include_paths, "libavutil/pixfmt.h"))
        .header(search_include(&include_paths, "libavutil/random_seed.h"))
        .header(search_include(&include_paths, "libavutil/rational.h"))
        .header(search_include(&include_paths, "libavutil/replaygain.h"))
        .header(search_include(&include_paths, "libavutil/ripemd.h"))
        .header(search_include(&include_paths, "libavutil/samplefmt.h"))
        .header(search_include(&include_paths, "libavutil/sha.h"))
        .header(search_include(&include_paths, "libavutil/sha512.h"))
        .header(search_include(&include_paths, "libavutil/stereo3d.h"))
        .header(search_include(&include_paths, "libavutil/avstring.h"))
        .header(search_include(&include_paths, "libavutil/threadmessage.h"))
        .header(search_include(&include_paths, "libavutil/time.h"))
        .header(search_include(&include_paths, "libavutil/timecode.h"))
        .header(search_include(&include_paths, "libavutil/twofish.h"))
        .header(search_include(&include_paths, "libavutil/avutil.h"))
        .header(search_include(&include_paths, "libavutil/xtea.h"));

    // The lzo may be disabled by `disable-everything`
    if let Some(path) = search_include_optional(&include_paths, "libavutil/lzo.h") {
        builder = builder.header(path);
    }

    if env::var("CARGO_FEATURE_POSTPROC").is_ok() {
        builder = builder.header(search_include(&include_paths, "libpostproc/postprocess.h"));
    }

    if env::var("CARGO_FEATURE_SWRESAMPLE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libswresample/swresample.h"));
    }

    if env::var("CARGO_FEATURE_SWSCALE").is_ok() {
        builder = builder.header(search_include(&include_paths, "libswscale/swscale.h"));
    }

    // Finish the builder and generate the bindings.
    let bindings = builder
        .generate()
        // Unwrap the Result and panic on failure.
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    bindings
        .write_to_file(output().join("bindings.rs"))
        .expect("Couldn't write bindings!");
}
