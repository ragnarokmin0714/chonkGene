//! The pet process's front door (PLAN §1.10, §3.2, §6 item 9): a second
//! launch and the settings process talk to it here.
//!
//! One RON message per line, at most `MAX_MESSAGE` bytes. Linux uses a
//! socket file in `$XDG_RUNTIME_DIR` (mode 0700, this user only); Windows a
//! named pipe, which other users cannot write to.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::thread;

use interprocess::local_socket::{Listener, ListenerOptions, Name, Stream, prelude::*};
use serde::{Deserialize, Serialize};

use crate::store::{PetSave, Settings};

const MAX_MESSAGE: u64 = 64 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Msg {
    /// A second launch: bring the pets back and open settings.
    Show,
    /// Replace the settings.
    Settings(Settings),
    /// Create or update a pet (matched by id).
    Pet(PetSave),
    /// Adopt a new pet of this species and coat.
    Adopt {
        species: String,
        coat: String,
    },
    /// Say goodbye to a pet for good.
    Release(u64),
    /// Remove start-at-login and menu entries (「移除所有痕跡」).
    RemoveTraces,
    Quit,
}

fn name() -> io::Result<Name<'static>> {
    #[cfg(windows)]
    {
        use interprocess::local_socket::GenericNamespaced;
        let user = std::env::var("USERNAME").unwrap_or_else(|_| "user".into());
        let clean: String = user.chars().filter(char::is_ascii_alphanumeric).collect();
        format!("chonkgene-{clean}")
            .to_ns_name::<GenericNamespaced>()
            .map(Name::into_owned)
    }
    #[cfg(not(windows))]
    {
        use interprocess::local_socket::GenericFilePath;
        let dir = std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .filter(|p| p.is_dir())
            .ok_or_else(|| io::Error::other("XDG_RUNTIME_DIR is not set"))?;
        dir.join("chonkgene.sock")
            .to_fs_name::<GenericFilePath>()
            .map(Name::into_owned)
    }
}

/// Send one message to the running pet process.
pub fn send(msg: &Msg) -> io::Result<()> {
    let mut stream = Stream::connect(name()?)?;
    let mut line = ron::to_string(msg).map_err(io::Error::other)?;
    line.push('\n');
    stream.write_all(line.as_bytes())
}

/// Become the pet process, or learn that one is already running.
pub enum Claim {
    Ours(Listener),
    AlreadyRunning,
}

pub fn claim() -> io::Result<Claim> {
    if send(&Msg::Show).is_ok() {
        return Ok(Claim::AlreadyRunning);
    }
    let options = || ListenerOptions::new().name(name().expect("name was built a moment ago"));
    match options().create_sync() {
        Ok(listener) => Ok(Claim::Ours(listener)),
        // A socket file left by a crash: nobody answered, so it is stale.
        Err(e) if e.kind() == io::ErrorKind::AddrInUse => {
            #[cfg(not(windows))]
            if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
                let _ = std::fs::remove_file(std::path::Path::new(&dir).join("chonkgene.sock"));
            }
            Ok(Claim::Ours(options().create_sync()?))
        }
        Err(e) => Err(e),
    }
}

/// Read messages on background threads and hand each to `deliver`.
pub fn serve(listener: Listener, deliver: impl Fn(Msg) -> bool + Clone + Send + 'static) {
    thread::spawn(move || {
        for conn in listener.incoming().filter_map(Result::ok) {
            let deliver = deliver.clone();
            thread::spawn(move || {
                let mut reader = BufReader::new(conn);
                let mut line = String::new();
                loop {
                    line.clear();
                    match (&mut reader).take(MAX_MESSAGE).read_line(&mut line) {
                        Ok(0) | Err(_) => break,
                        Ok(_) => match ron::from_str::<Msg>(line.trim_end()) {
                            Ok(msg) => {
                                if !deliver(msg) {
                                    break;
                                }
                            }
                            Err(e) => eprintln!("ignored a message that did not parse: {e}"),
                        },
                    }
                }
            });
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_survive_the_wire_format() {
        let mut rng = fastrand::Rng::with_seed(1);
        let mut pet = PetSave::adopt(7, chonkgene_core::species::Species::Duck, "white", &mut rng);
        pet.name = "湯圓\n第二行".into();
        for msg in [
            Msg::Show,
            Msg::Settings(Settings::default()),
            Msg::Pet(pet),
            Msg::Adopt {
                species: "owl".into(),
                coat: "brown".into(),
            },
            Msg::Release(3),
        ] {
            let line = ron::to_string(&msg).unwrap();
            assert!(!line.contains('\n'), "a message must fit on one line");
            assert_eq!(ron::from_str::<Msg>(&line).unwrap(), msg);
        }
    }
}
