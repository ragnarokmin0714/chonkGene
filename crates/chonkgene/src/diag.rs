//! A log file for what cannot be seen on Windows, where release builds have
//! no console (PLAN §8, Roger's eleventh pass, item 5): the event loop held
//! up, a pet whose picture stopped changing, and every click on a pet with
//! what came of it. `chonkgene.log` in the data folder, started afresh each
//! run and kept under [`MOST`] bytes; every line also goes to the console.

use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The log stops growing at this size (a long run with something going
/// wrong every frame).
const MOST: u64 = 2 * 1024 * 1024;

static LOG: Mutex<Option<(File, u64)>> = Mutex::new(None);
static START: OnceLock<Instant> = OnceLock::new();

/// Start the log in `dir` (the data folder).
pub fn open(dir: &Path) {
    START.get_or_init(Instant::now);
    match File::create(dir.join("chonkgene.log")) {
        Ok(f) => {
            if let Ok(mut log) = LOG.lock() {
                *log = Some((f, 0));
            }
        }
        Err(e) => eprintln!("no log file: {e}"),
    }
}

/// One line, stamped with the seconds since start.
pub fn line(msg: std::fmt::Arguments) {
    let secs = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let text = format!("[{secs:9.3}] {msg}\n");
    print!("{text}");
    if let Ok(mut log) = LOG.lock()
        && let Some((f, n)) = log.as_mut()
        && *n < MOST
    {
        *n += text.len() as u64;
        let _ = f.write_all(text.as_bytes());
    }
}

/// [`line`] with `format!` arguments.
macro_rules! diag {
    ($($arg:tt)*) => {
        $crate::diag::line(format_args!($($arg)*))
    };
}
pub(crate) use diag;
