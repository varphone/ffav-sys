use crate::{AVRational, AVRounding};

impl Default for AVRational {
    fn default() -> Self {
        AVRational { den: 0, num: 0 }
    }
}

impl From<AVRounding> for u32 {
    fn from(v: AVRounding) -> u32 {
        unsafe { std::mem::transmute::<AVRounding, u32>(v) }
    }
}

impl Default for AVRounding {
    fn default() -> Self {
        AVRounding::new()
    }
}

impl AVRounding {
    /// Create an new AVRounding with Round toward zero.
    #[inline]
    pub fn new() -> Self {
        AVRounding::AV_ROUND_ZERO
    }

    /// Round toward zero.
    #[inline]
    pub fn zero(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(0) }
    }

    /// Round away from zero.
    #[inline]
    pub fn inf(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(self as u32 | 1) }
    }

    /// Round toward -infinity.
    #[inline]
    pub fn down(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(self as u32 | 2) }
    }

    /// Round toward +infinity.
    #[inline]
    pub fn up(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(self as u32 | 3) }
    }

    /// Round to nearest and halfway cases away from zero.
    #[inline]
    pub fn near_inf(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(self as u32 | 5) }
    }

    /// Flag telling rescaling functions to pass INT64_MIN/MAX through unchanged, avoiding special cases for AV_NOPTS_VALUE.
    ///
    /// Unlike other values of the enumeration AVRounding, this value is a bitmask that must be used in conjunction with another value of the enumeration through a bitwise OR, in order to set behavior for normal cases.
    #[inline]
    pub fn pass_min_max(self) -> Self {
        unsafe { std::mem::transmute::<u32, AVRounding>(self as u32 | 8192) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_avrounding() {
        assert_eq!(
            std::mem::size_of::<AVRounding>(),
            std::mem::size_of::<u32>()
        );
        assert_eq!(AVRounding::new(), AVRounding::AV_ROUND_ZERO);
        assert_eq!(AVRounding::new().zero(), AVRounding::AV_ROUND_ZERO);
        assert_eq!(AVRounding::new().inf(), AVRounding::AV_ROUND_INF);
        assert_eq!(AVRounding::new().down(), AVRounding::AV_ROUND_DOWN);
        assert_eq!(AVRounding::new().up(), AVRounding::AV_ROUND_UP);
        assert_eq!(AVRounding::new().near_inf(), AVRounding::AV_ROUND_NEAR_INF);
        assert_eq!(
            AVRounding::new().pass_min_max(),
            AVRounding::AV_ROUND_PASS_MINMAX
        );
        assert_eq!(AVRounding::new().near_inf().pass_min_max() as u32, 8197);
    }
}
