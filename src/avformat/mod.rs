use crate::{
    AVChapter, AVCodecContext, AVCodecParameters, AVDictionary, AVFormatContext, AVPacketSideData,
    AVProgram, AVStream,
};
use std::convert::TryInto;

impl AVFormatContext {
    /// Number of elements in AVFormatContext.streams.
    #[inline]
    pub fn nb_streams(&self) -> usize {
        self.nb_streams as usize
    }

    /// A list of all streams in the file.
    #[inline]
    pub fn streams(&self) -> &[&AVStream] {
        unsafe { std::slice::from_raw_parts(self.streams as *const &AVStream, self.nb_streams()) }
    }

    /// A list of all streams in the file.
    #[inline]
    pub fn streams_mut(&self) -> &[&mut AVStream] {
        unsafe {
            std::slice::from_raw_parts(self.streams as *const &mut AVStream, self.nb_streams())
        }
    }

    /// Number of elements in AVFormatContext.programs.
    #[inline]
    pub fn nb_programs(&self) -> usize {
        self.nb_programs as usize
    }

    /// A list of all programs in the file.
    #[inline]
    pub fn programs(&self) -> &[&AVProgram] {
        unsafe {
            std::slice::from_raw_parts(self.programs as *const &AVProgram, self.nb_programs())
        }
    }

    /// A list of all programs in the file.
    #[inline]
    pub fn programs_mut(&self) -> &[&mut AVProgram] {
        unsafe {
            std::slice::from_raw_parts(self.programs as *const &mut AVProgram, self.nb_programs())
        }
    }

    /// Number of elements in AVFormatContext.chapters.
    #[inline]
    pub fn nb_chapters(&self) -> usize {
        self.nb_chapters as usize
    }

    /// A list of all chapters in the file.
    #[inline]
    pub fn chapters(&self) -> &[&AVChapter] {
        unsafe {
            std::slice::from_raw_parts(self.chapters as *const &AVChapter, self.nb_chapters())
        }
    }

    /// A list of all chapters in the file.
    #[inline]
    pub fn chapters_mut(&self) -> &[&mut AVChapter] {
        unsafe {
            std::slice::from_raw_parts(self.chapters as *const &mut AVChapter, self.nb_chapters())
        }
    }
}

impl AVStream {
    #[inline]
    pub fn codec(&self) -> Option<&AVCodecContext> {
        if self.codec.is_null() {
            None
        } else {
            unsafe { Some(&*self.codec) }
        }
    }

    #[inline]
    pub fn codecpar(&self) -> Option<&AVCodecParameters> {
        if self.codecpar.is_null() {
            None
        } else {
            unsafe { Some(&*self.codecpar) }
        }
    }

    #[inline]
    pub fn metadata(&self) -> Option<&AVDictionary> {
        if self.metadata.is_null() {
            None
        } else {
            unsafe { Some(&*self.metadata) }
        }
    }

    #[inline]
    pub fn side_data(&self) -> &[AVPacketSideData] {
        if self.side_data.is_null() || self.nb_side_data <= 0 {
            &[]
        } else {
            unsafe {
                std::slice::from_raw_parts(self.side_data, self.nb_side_data.try_into().unwrap())
            }
        }
    }
}
