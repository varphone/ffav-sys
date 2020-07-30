use crate::{av_q2d, AVRational, AV_NOPTS_VALUE};

pub fn av_ts2str(ts: i64) -> String {
    if ts == AV_NOPTS_VALUE {
        "NOPTS".to_string()
    } else {
        ts.to_string()
    }
}

pub fn av_ts2timestr(ts: i64, tb: &AVRational) -> String {
    if ts == AV_NOPTS_VALUE {
        "NOPTS".to_string()
    } else {
        unsafe { (av_q2d(*tb) * ts as f64).to_string() }
    }
}
