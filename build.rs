//! Emit `CUSTOM_UTILS_BUILD_DATE` (UTC, `YYYY-MM-DD`) for `env!()` in the
//! library. Captures the library's compile time, which — in the normal case
//! where the host binary is built in the same session — matches the binary's
//! build date.
//!
//! No `rerun-if-*` directives are emitted on purpose: Cargo's default is to
//! rerun on any source change in the package, so the date refreshes whenever
//! the crate is recompiled.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (y, m, d) = ymd_from_unix(secs);
    println!("cargo:rustc-env=CUSTOM_UTILS_BUILD_DATE={y:04}-{m:02}-{d:02}");
}

/// Howard Hinnant's `civil_from_days` algorithm: Unix seconds → (Y, M, D) in UTC.
fn ymd_from_unix(secs: u64) -> (i64, u32, u32) {
    let days = (secs / 86_400) as i64;
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097; // [0, 146096]
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let y = if m <= 2 { y + 1 } else { y };
    (y, m as u32, d as u32)
}
