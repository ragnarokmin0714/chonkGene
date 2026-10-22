//! S3: the settings UI in its own process (PLAN §3.2), on Slint, talking to
//! the pet process over IPC. Checks Chinese IME, a frameless rounded window,
//! Slint's animation feel, and that a crash here leaves the pets running.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use slint::ComponentHandle;
use slint::winit_030::WinitWindowAccessor;

slint::include_modules!();

pub fn run(sock: &str) -> Result<(), String> {
    let link = Rc::new(RefCell::new(match crate::ipc::connect(sock) {
        Ok(stream) => Some(stream),
        Err(e) => {
            eprintln!("settings: cannot reach the pet process: {e}");
            None
        }
    }));
    let send = move |msg: &str| {
        if let Some(stream) = link.borrow_mut().as_mut()
            && let Err(e) = writeln!(stream, "{msg}")
        {
            eprintln!("settings: send failed: {e}");
        }
    };
    send("hello from settings");

    let ui = SettingsWindow::new().map_err(|e| e.to_string())?;
    {
        let send = send.clone();
        ui.on_send_name(move |name| send(&format!("name {name}")));
    }
    ui.on_crash(|| std::process::abort());
    {
        let weak = ui.as_weak();
        ui.on_close_window(move || {
            if let Some(ui) = weak.upgrade() {
                let _ = ui.hide();
            }
        });
    }
    {
        let weak = ui.as_weak();
        ui.on_drag(move || {
            if let Some(ui) = weak.upgrade() {
                ui.window().with_winit_window(|w| {
                    let _ = w.drag_window();
                });
            }
        });
    }
    ui.run().map_err(|e| e.to_string())?;
    send("bye");
    Ok(())
}
