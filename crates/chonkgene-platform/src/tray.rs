//! Tray icon (PLAN §1.10): Windows notification area, Linux
//! StatusNotifierItem (KDE, and GNOME with the AppIndicator extension).
//! Without a tray, everything is still in the pet's right-click menu.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    /// Hide all pets, or bring them back.
    Toggle,
    Settings,
    Quit,
}

const TOOLTIP: &str = "肉肉賴桌";
const LABELS: [(&str, TrayCommand); 3] = [
    ("收起來 / 叫回來", TrayCommand::Toggle),
    ("設定…", TrayCommand::Settings),
    ("結束肉肉賴桌", TrayCommand::Quit),
];

/// Keeps the tray icon alive; dropping it removes the icon.
pub struct Tray {
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
    #[cfg(target_os = "linux")]
    _handle: ksni::blocking::Handle<Sni>,
}

/// `rgba` is a straight-alpha RGBA image of `size` × `size` pixels.
pub fn create(
    rgba: Vec<u8>,
    size: u32,
    on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
) -> Result<Tray, String> {
    imp::create(rgba, size, on)
}

#[cfg(windows)]
mod imp {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    use super::{LABELS, TOOLTIP, Tray, TrayCommand};

    pub fn create(
        rgba: Vec<u8>,
        size: u32,
        on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
    ) -> Result<Tray, String> {
        let menu = Menu::new();
        let mut ids = Vec::new();
        for (i, (label, command)) in LABELS.iter().enumerate() {
            if i == LABELS.len() - 1 {
                menu.append(&PredefinedMenuItem::separator())
                    .map_err(|e| e.to_string())?;
            }
            let item = MenuItem::new(*label, true, None);
            ids.push((item.id().clone(), *command));
            menu.append(&item).map_err(|e| e.to_string())?;
        }
        let icon = Icon::from_rgba(rgba, size, size).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(TOOLTIP)
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        let on_menu = on.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            if let Some((_, command)) = ids.iter().find(|(id, _)| *id == e.id) {
                on_menu(*command);
            }
        }));
        TrayIconEvent::set_event_handler(Some(move |e: TrayIconEvent| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = e
            {
                on(TrayCommand::Toggle);
            }
        }));
        Ok(Tray { _icon: tray })
    }
}

#[cfg(target_os = "linux")]
pub struct Sni {
    icon: ksni::Icon,
    on: Box<dyn Fn(TrayCommand) + Send + Sync>,
}

#[cfg(target_os = "linux")]
impl ksni::Tray for Sni {
    fn id(&self) -> String {
        "chonkgene".into()
    }

    fn title(&self) -> String {
        TOOLTIP.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        vec![self.icon.clone()]
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on)(TrayCommand::Toggle);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let mut items = Vec::new();
        for (i, (label, command)) in LABELS.iter().enumerate() {
            if i == LABELS.len() - 1 {
                items.push(ksni::MenuItem::Separator);
            }
            let command = *command;
            items.push(
                ksni::menu::StandardItem {
                    label: (*label).into(),
                    activate: Box::new(move |this: &mut Self| (this.on)(command)),
                    ..Default::default()
                }
                .into(),
            );
        }
        items
    }
}

#[cfg(target_os = "linux")]
mod imp {
    use ksni::blocking::TrayMethods;

    use super::{Sni, Tray, TrayCommand};

    pub fn create(
        rgba: Vec<u8>,
        size: u32,
        on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
    ) -> Result<Tray, String> {
        // StatusNotifierItem wants ARGB32 in network byte order.
        let (px, _) = rgba.as_chunks::<4>();
        let data = px.iter().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect();
        let sni = Sni {
            icon: ksni::Icon {
                width: size as i32,
                height: size as i32,
                data,
            },
            on: Box::new(on),
        };
        let handle = sni.spawn().map_err(|e| e.to_string())?;
        Ok(Tray { _handle: handle })
    }
}
