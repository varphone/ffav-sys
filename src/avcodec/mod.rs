use crate::{
    AVCodecContext, AVCodecID, AVMediaType, AVPacket, AVPacketSideData, AVPixelFormat,
    AVSampleFormat, AV_NOPTS_VALUE,
};
use std::borrow::Cow;
use std::convert::TryInto;
use std::ffi::CStr;

impl AVCodecContext {
    /// Some codecs need / can use extradata like Huffman tables.
    #[inline]
    pub fn extradata(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(self.extradata, self.extradata_size.try_into().unwrap())
        }
    }

    /// Additional data associated with the entire coded stream.
    #[inline]
    pub fn coded_side_data(&self) -> &[AVPacketSideData] {
        if self.coded_side_data.is_null() || self.nb_coded_side_data <= 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(self.coded_side_data, self.nb_coded_side_data as usize)
            }
        }
    }
}

impl Default for AVCodecID {
    fn default() -> Self {
        AVCodecID::AV_CODEC_ID_NONE
    }
}

impl AVCodecID {
    /// Get the type of the given codec.
    pub fn get_type(self) -> AVMediaType {
        unsafe { crate::avcodec_get_type(self) }
    }

    /// Get the name of the given codec.
    pub fn get_name(self) -> Cow<'static, str> {
        unsafe {
            let name = crate::avcodec_get_name(self);
            if name.is_null() {
                Cow::Borrowed("<Unknown>")
            } else {
                CStr::from_ptr(name).to_string_lossy()
            }
        }
    }

    /// Return true if the given codec has GOP props.
    /// # Notes
    /// The types annotations is incomplete.
    pub fn has_gop(self) -> bool {
        use AVCodecID::*;
        matches!(
            self,
            AV_CODEC_ID_H264 | AV_CODEC_ID_HEVC | AV_CODEC_ID_VP8 | AV_CODEC_ID_VP9
        )
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

impl AVPacket {
    /// Return a empty packet.
    pub fn empty() -> Self {
        Default::default()
    }

    /// Returns true if data bytes has a length of zero bytes.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the length of data bytes.
    pub fn len(&self) -> usize {
        self.size as usize
    }

    /// Converts a data ptr to a byte slice.
    pub fn as_bytes(&self) -> &[u8] {
        unsafe { std::slice::from_raw_parts(self.data, self.size as usize) }
    }

    /// Converts a mutable data ptr to a mutable byte slice.
    pub fn as_bytes_mut(&mut self) -> &mut [u8] {
        unsafe { std::slice::from_raw_parts_mut(self.data, self.size as usize) }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avpacket() {
        let mut pkt = AVPacket::default();
        assert_eq!(pkt.is_empty(), true);
        assert_eq!(pkt.len(), 0);
        assert_eq!(pkt.as_bytes(), &[]);
        assert_eq!(pkt.as_bytes_mut(), &[]);
    }
}
