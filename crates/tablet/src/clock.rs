//! A single monotonic clock for every input source, so samples from the mouse
//! and the pen (and the moment they were processed) can be compared.
//!
//! On Windows this is the performance counter, the same clock the pointer
//! stack stamps pen samples with.

/// Seconds since an arbitrary fixed origin.
#[cfg(windows)]
pub fn now() -> f64 {
    use windows_sys::Win32::System::Performance::QueryPerformanceCounter;
    let mut count = 0i64;
    // SAFETY: the pointer is valid for the duration of the call.
    #[allow(unsafe_code)]
    unsafe {
        QueryPerformanceCounter(&mut count);
    }
    count as f64 / qpc_frequency()
}

/// Ticks per second of the performance counter.
#[cfg(windows)]
pub fn qpc_frequency() -> f64 {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::Performance::QueryPerformanceFrequency;
    static FREQ: OnceLock<f64> = OnceLock::new();
    *FREQ.get_or_init(|| {
        let mut f = 0i64;
        // SAFETY: the pointer is valid for the duration of the call.
        #[allow(unsafe_code)]
        unsafe {
            QueryPerformanceFrequency(&mut f);
        }
        f.max(1) as f64
    })
}

#[cfg(not(windows))]
pub fn now() -> f64 {
    use std::sync::OnceLock;
    use std::time::Instant;
    static ORIGIN: OnceLock<Instant> = OnceLock::new();
    ORIGIN.get_or_init(Instant::now).elapsed().as_secs_f64()
}

/// Makes timestamps strictly increasing within a stroke.
///
/// Devices batch samples that can share a timestamp (millisecond clocks) or,
/// across sources, arrive slightly out of order. Downstream stages divide by
/// `dt`, so equal or decreasing times are nudged forward by `min_step`.
#[derive(Clone, Debug)]
pub struct TimestampNormalizer {
    pub min_step: f64,
    last: Option<f64>,
}

impl Default for TimestampNormalizer {
    fn default() -> Self {
        Self {
            min_step: 0.000_1,
            last: None,
        }
    }
}

impl TimestampNormalizer {
    pub fn reset(&mut self) {
        self.last = None;
    }

    pub fn normalize(&mut self, t: f64) -> f64 {
        let t = match self.last {
            Some(last) if t.is_nan() || t <= last => last + self.min_step,
            _ => t,
        };
        self.last = Some(t);
        t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_is_monotonic() {
        let a = now();
        let b = now();
        assert!(b >= a);
    }

    #[test]
    fn timestamps_become_strictly_increasing() {
        let mut n = TimestampNormalizer::default();
        let out: Vec<f64> = [1.0, 1.0, 1.0, 0.9, 1.5, f64::NAN, 2.0]
            .into_iter()
            .map(|t| n.normalize(t))
            .collect();
        assert!(out.windows(2).all(|w| w[1] > w[0]), "{out:?}");
        assert_eq!(out[0], 1.0);
        assert_eq!(out[4], 1.5);
        assert_eq!(out[6], 2.0);
    }

    #[test]
    fn reset_forgets_previous_stroke() {
        let mut n = TimestampNormalizer::default();
        n.normalize(10.0);
        n.reset();
        assert_eq!(n.normalize(1.0), 1.0);
    }
}
