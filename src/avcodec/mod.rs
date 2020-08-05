use crate::{AVCodecID, AVPacket, AVPixelFormat, AVSampleFormat, AV_NOPTS_VALUE};

impl Default for AVCodecID {
    fn default() -> Self {
        AVCodecID::AV_CODEC_ID_NONE
    }
}

impl Default for AVPacket {
    fn default() -> Self {
        Self {
            buf: std::ptr::null_mut(),
            pts: AV_NOPTS_VALUE,
            dts: AV_NOPTS_VALUE,
            data: std::ptr::null_mut(),
            size: 0,
            stream_index: 0,
            flags: 0,
            side_data: std::ptr::null_mut(),
            side_data_elems: 0,
            duration: 0,
            pos: -1,
            convergence_duration: 0,
        }
    }
}

impl Default for AVPixelFormat {
    fn default() -> Self {
        AVPixelFormat::AV_PIX_FMT_NONE
    }
}

impl Default for AVSampleFormat {
    fn default() -> Self {
        AVSampleFormat::AV_SAMPLE_FMT_NONE
    }
}
