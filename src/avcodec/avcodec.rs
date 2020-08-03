use crate::{AVCodecID,AVPixelFormat,AVSampleFormat};

impl Default for AVCodecID {
    fn default() -> Self {
        AVCodecID::AV_CODEC_ID_NONE
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
