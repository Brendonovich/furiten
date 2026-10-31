//! A dependency-free C ABI for the Worker. Reviews go in as `[reviewed_at, rating]` f64 pairs (ms timestamps fit an
//! f64 exactly) and the schedule comes out as 7 f64s:
//! `[state (0 new, 1 learning, 2 review, 3 relearning), due, stability, difficulty, reps, lapses, last_review (-1 = none)]`.

use crate::{Rating, Scheduler, State};

#[unsafe(no_mangle)]
pub extern "C" fn srs_alloc(len: usize) -> *mut f64 {
    let mut buf = Vec::<f64>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    core::mem::forget(buf);
    ptr
}

/// # Safety
/// `ptr` must come from `srs_alloc(len)` and not be freed twice.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn srs_free(ptr: *mut f64, len: usize) {
    drop(unsafe { Vec::from_raw_parts(ptr, 0, len) });
}

/// Returns 0 on success, 1 on an invalid rating.
///
/// # Safety
/// `reviews` holds `2 * count` f64s and `out` holds 7.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn srs_fold(created_at: f64, reviews: *const f64, count: usize, out: *mut f64) -> u32 {
    let pairs = unsafe { core::slice::from_raw_parts(reviews, count * 2) };
    let parsed: Option<Vec<(i64, Rating)>> =
        pairs.chunks_exact(2).map(|p| Rating::from_u8(p[1] as u8).map(|r| (p[0] as i64, r))).collect();
    let Some(parsed) = parsed else { return 1 };
    let s = Scheduler::default().fold(created_at as i64, parsed);
    let out = unsafe { core::slice::from_raw_parts_mut(out, 7) };
    out[0] = match s.state {
        State::New => 0.0,
        State::Learning => 1.0,
        State::Review => 2.0,
        State::Relearning => 3.0,
    };
    out[1] = s.due as f64;
    out[2] = s.stability;
    out[3] = s.difficulty;
    out[4] = s.reps as f64;
    out[5] = s.lapses as f64;
    out[6] = s.last_review.map_or(-1.0, |t| t as f64);
    0
}
