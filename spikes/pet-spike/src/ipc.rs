//! Pet process ↔ settings process channel: newline-separated text messages
//! over a local socket (named pipe on Windows).
//!
//! Spike shortcut: the name is in the generic namespace (an abstract socket
//! on Linux). PLAN §6 item 9 wants a socket file under `$XDG_RUNTIME_DIR` and
//! a current-user-only pipe; Phase 2 does that.

use std::io::{self, BufRead, BufReader, Read};
use std::thread;

use interprocess::local_socket::{GenericNamespaced, ListenerOptions, Stream, prelude::*};
use winit::event_loop::EventLoopProxy;

use crate::app::UserEvent;

/// Longest message we accept; anything longer is cut.
const MAX_LINE: u64 = 4096;

/// Starts the listener and returns the name to hand to the settings process.
pub fn serve(proxy: EventLoopProxy<UserEvent>) -> io::Result<String> {
    let printname = format!("chonkgene-spike-{}.sock", std::process::id());
    let listener = ListenerOptions::new()
        .name(printname.as_str().to_ns_name::<GenericNamespaced>()?)
        .create_sync()?;
    thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let proxy = proxy.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(conn);
                let mut line = String::new();
                loop {
                    line.clear();
                    match (&mut reader).take(MAX_LINE).read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => {
                            let msg = line.trim_end().to_owned();
                            if proxy.send_event(UserEvent::Ipc(msg)).is_err() {
                                return;
                            }
                        }
                    }
                }
                let _ = proxy.send_event(UserEvent::Ipc("<settings disconnected>".into()));
            });
        }
    });
    Ok(printname)
}

pub fn connect(printname: &str) -> io::Result<Stream> {
    Stream::connect(printname.to_ns_name::<GenericNamespaced>()?)
}
