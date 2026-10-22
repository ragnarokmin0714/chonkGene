//! The global hotkey Ctrl+Alt+P (PLAN §1.10): RegisterHotKey on Windows,
//! XGrabKey on X11. A key binding, not an input hook: nothing else is seen.

use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

/// Keeps the hotkey registered; dropping it unregisters.
pub struct Hotkey {
    _manager: GlobalHotKeyManager,
}

/// Register Ctrl+Alt+P; `on` runs on another thread when it is pressed.
pub fn register(on: impl Fn() + Send + Sync + 'static) -> Result<Hotkey, String> {
    let manager = GlobalHotKeyManager::new().map_err(|e| e.to_string())?;
    let key = HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT), Code::KeyP);
    manager.register(key).map_err(|e| e.to_string())?;
    let id = key.id();
    GlobalHotKeyEvent::set_event_handler(Some(move |e: GlobalHotKeyEvent| {
        if e.id == id && e.state == HotKeyState::Pressed {
            on();
        }
    }));
    Ok(Hotkey { _manager: manager })
}
