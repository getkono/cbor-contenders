//! What this build and this CPU will actually run, and how quiet the host is.

use std::time::Duration;

use serde_json::{Value, json};

/// Target features this binary was compiled with. No CBOR candidate picks a
/// SIMD path by them; they are what the compiler may auto-vectorize with, and
/// `xtask doctor` checks they are exactly the variant's.
fn compiled_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    macro_rules! probe {
        ($($f:literal),*) => { $( if cfg!(target_feature = $f) { features.push($f); } )* };
    }
    probe!(
        "sse2",
        "ssse3",
        "sse4.1",
        "sse4.2",
        "popcnt",
        "avx",
        "avx2",
        "bmi1",
        "bmi2",
        "fma",
        "lzcnt",
        "pclmulqdq",
        "avx512f",
        "avx512bw"
    );
    features
}

/// Features the CPU reports at run time.
fn detected_features() -> Vec<&'static str> {
    let mut features = Vec::new();
    macro_rules! detect {
        ($($f:tt),*) => { $( if std::is_x86_feature_detected!($f) { features.push($f); } )* };
    }
    detect!("sse4.2", "avx2", "bmi2", "pclmulqdq", "avx512f", "avx512bw");
    features
}

/// The doctor report.
pub fn report() -> Value {
    json!({
        "arch": std::env::consts::ARCH,
        "os": std::env::consts::OS,
        "compiled_features": compiled_features(),
        "detected_features": detected_features(),
        "backends": codecs::compiled(),
    })
}

/// Pin this thread to `core`. Returns whether it worked.
pub fn pin(core: usize) -> bool {
    // SAFETY: `set` is a zeroed, then initialized, cpu_set_t owned by this
    // frame; the call reads it and nothing else.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_SET(core, &mut set);
        libc::sched_setaffinity(0, size_of::<libc::cpu_set_t>(), &raw const set) == 0
    }
}

fn thread_cpu() -> Duration {
    let mut ts = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    // SAFETY: `ts` is a valid, writable timespec owned by this frame.
    unsafe {
        libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &raw mut ts);
    }
    Duration::new(ts.tv_sec as u64, ts.tv_nsec as u32)
}

/// Measures how much of a sample window something other than this thread
/// spent on the CPU the sample needed. Only the pinned core is read here:
/// `/proc/schedstat` credits a task's run time when it is switched out, so a
/// task that keeps another CPU busy without switching shows nothing there.
/// The SMT sibling is checked by `xtask` around each cell instead.
#[derive(Debug)]
pub struct Interference {
    core: Option<usize>,
    busy: Option<u64>,
    own: Duration,
}

impl Interference {
    /// How interference is detected.
    pub fn method() -> &'static str {
        "per-core run time from /proc/schedstat minus this thread's CPU time"
    }

    /// Start observing. `core` is the CPU the thread is pinned to, if any.
    pub fn start(core: Option<usize>) -> Self {
        Self {
            core,
            busy: busy_ns(core),
            own: thread_cpu(),
        }
    }

    /// The fraction of `window` spent by other work.
    pub fn finish(self, window: Duration) -> f64 {
        let own = thread_cpu().saturating_sub(self.own);
        match (self.busy, busy_ns(self.core)) {
            (Some(before), Some(after)) => {
                let other = after.saturating_sub(before) as f64 - own.as_nanos() as f64;
                (other / window.as_nanos() as f64).max(0.0)
            }
            _ => 0.0,
        }
    }
}

/// Nanoseconds tasks have run on `core`.
fn busy_ns(core: Option<usize>) -> Option<u64> {
    let core = core?;
    let stat = std::fs::read_to_string("/proc/schedstat").ok()?;
    let line = stat.lines().find(|l| l.starts_with(&format!("cpu{core} ")))?;
    // cpuN <9 legacy fields> <run time ns> <wait time ns> <timeslices>
    line.split_whitespace().nth(7)?.parse().ok()
}
