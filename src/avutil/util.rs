use crate::{AVMediaType, AVRational, AV_TIME_BASE, MKTAG};
use libc::c_int;

pub const AV_NOPTS_VALUE: i64 = 0x8000000000000000u64 as i64;
pub const AV_TIME_BASE_Q: AVRational = AVRational {
    num: 1,
    den: AV_TIME_BASE as c_int,
};

pub const AV_CODEC_TAG_AVC1: u32 = MKTAG!(b'a', b'v', b'c', b'1') as u32;
pub const AV_CODEC_TAG_HEV1: u32 = MKTAG!(b'h', b'e', b'v', b'1') as u32;
pub const AV_CODEC_TAG_HVC1: u32 = MKTAG!(b'h', b'v', b'c', b'1') as u32;

impl Default for AVMediaType {
    fn default() -> Self {
        AVMediaType::AVMEDIA_TYPE_UNKNOWN
    }
}
