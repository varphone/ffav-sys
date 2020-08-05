use crate::{AVChapter, AVFormatContext, AVProgram, AVStream};

impl AVFormatContext {
    #[inline]
    pub fn nb_streams(&self) -> usize {
        self.nb_streams as usize
    }

    #[inline]
    pub fn streams(&self) -> &[&AVStream] {
        unsafe { std::slice::from_raw_parts(self.streams as *const &AVStream, self.nb_streams()) }
    }

    #[inline]
    pub fn streams_mut(&self) -> &[&mut AVStream] {
        unsafe {
            std::slice::from_raw_parts(self.streams as *const &mut AVStream, self.nb_streams())
        }
    }

    #[inline]
    pub fn nb_programs(&self) -> usize {
        self.nb_programs as usize
    }

    #[inline]
    pub fn programs(&self) -> &[&AVProgram] {
        unsafe {
            std::slice::from_raw_parts(self.programs as *const &AVProgram, self.nb_programs())
        }
    }

    #[inline]
    pub fn programs_mut(&self) -> &[&mut AVProgram] {
        unsafe {
            std::slice::from_raw_parts(self.programs as *const &mut AVProgram, self.nb_programs())
        }
    }

    #[inline]
    pub fn nb_chapters(&self) -> usize {
        self.nb_chapters as usize
    }

    #[inline]
    pub fn chapters(&self) -> &[&AVChapter] {
        unsafe {
            std::slice::from_raw_parts(self.chapters as *const &AVChapter, self.nb_chapters())
        }
    }

    #[inline]
    pub fn chapters_mut(&self) -> &[&mut AVChapter] {
        unsafe {
            std::slice::from_raw_parts(self.chapters as *const &mut AVChapter, self.nb_chapters())
        }
    }
}
