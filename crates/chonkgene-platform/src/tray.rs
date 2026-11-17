//! Tray icon (PLAN §1.10): Windows notification area, Linux
//! StatusNotifierItem (KDE, and GNOME with the AppIndicator extension).
//! Without a tray, everything is still in the pet's right-click menu.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayCommand {
    /// Hide all pets, or bring them back.
    Toggle,
    /// Take out the grass stalk to tease the pets with, or put it away.
    Toy,
    /// Throw a ball, or a frisbee, for the pets.
    Ball,
    Frisbee,
    /// Put down a sprig of catnip.
    Catnip,
    /// Every pet this size at once, percent (大家的大小).
    Size(u16),
    /// A scene, by its place in [`SCENES`]: called up for a pet that can,
    /// or put away if one is in it.
    Scene(u8),
    Settings,
    Quit,
}

const TOOLTIP: &str = mark("肉肉賴桌");
const LABELS: [(&str, TrayCommand); 7] = [
    (mark("收起來 / 叫回來"), TrayCommand::Toggle),
    (mark("拿狗尾草逗牠們 / 收起來"), TrayCommand::Toy),
    (mark("丟球給牠們"), TrayCommand::Ball),
    (mark("丟飛盤給牠們"), TrayCommand::Frisbee),
    (mark("放一把貓薄荷"), TrayCommand::Catnip),
    (mark("設定…"), TrayCommand::Settings),
    (mark("結束肉肉賴桌"), TrayCommand::Quit),
];

/// The 大家的大小 submenu, after the things to play with: label and percent
/// (the pet menu and settings offer the same).
const SIZE_MENU: &str = mark("大家的大小");
pub const SIZES: [(&str, u16); 4] = [
    (mark("小"), 70),
    (mark("中"), 100),
    (mark("大"), 130),
    (mark("特大"), 160),
];
/// The 場景 submenu, before 大家的大小: the pet menu's 「場景…」 rows (the
/// app maps each place to its scene).
const SCENE_MENU: &str = mark("場景");
pub const SCENES: [&str; 7] = [
    mark("玩水池"),
    mark("曬太陽"),
    mark("追蝴蝶"),
    mark("啃骨頭"),
    mark("啄種子"),
    mark("飛上樹枝"),
    mark("狐火"),
];
/// The submenus go before this entry of [`LABELS`] (設定…).
const SUBMENUS_BEFORE: usize = 5;

/// The submenus, in order: title and entries.
fn submenus() -> [(&'static str, Vec<(&'static str, TrayCommand)>); 2] {
    [
        (
            SCENE_MENU,
            SCENES
                .iter()
                .enumerate()
                .map(|(i, &label)| (label, TrayCommand::Scene(i as u8)))
                .collect(),
        ),
        (
            SIZE_MENU,
            SIZES
                .iter()
                .map(|&(label, pct)| (label, TrayCommand::Size(pct)))
                .collect(),
        ),
    ]
}

/// Which command each tray menu item stands for, and where commands go:
/// replaced whole each time the tray is made (again on a change of
/// language), read by event handlers set only once. tray-icon's and muda's
/// handlers live in a `OnceLock`, so a second `set_event_handler` is
/// ignored: a handler holding the first menu's ids matched none of the
/// remade menu's items, and every tray command did nothing after the
/// language changed (Roger, 2026-11-17).
#[cfg(any(windows, test))]
pub(crate) struct Routes<Id> {
    ids: Vec<(Id, TrayCommand)>,
    on: Option<Box<dyn Fn(TrayCommand) + Send + Sync>>,
}

#[cfg(any(windows, test))]
impl<Id: PartialEq> Routes<Id> {
    pub(crate) const fn new() -> Self {
        Self {
            ids: Vec::new(),
            on: None,
        }
    }

    /// The items of the menu now showing, and where their commands go.
    pub(crate) fn set(
        &mut self,
        ids: Vec<(Id, TrayCommand)>,
        on: impl Fn(TrayCommand) + Send + Sync + 'static,
    ) {
        self.ids = ids;
        self.on = Some(Box::new(on));
    }

    /// Send the command of the item `id`, if it is one of this menu's.
    pub(crate) fn chosen(&self, id: &Id) -> bool {
        let Some((_, command)) = self.ids.iter().find(|(i, _)| i == id) else {
            return false;
        };
        self.send(*command);
        true
    }

    pub(crate) fn send(&self, command: TrayCommand) {
        if let Some(on) = &self.on {
            on(command);
        }
    }
}

/// Keeps the tray icon alive; dropping it removes the icon.
pub struct Tray {
    #[cfg(windows)]
    _icon: tray_icon::TrayIcon,
    #[cfg(target_os = "linux")]
    _handle: ksni::blocking::Handle<Sni>,
}

/// Each icon is a straight-alpha RGBA image and its side in pixels. Windows
/// shows the first; a StatusNotifierItem host picks the size it needs.
pub fn create(
    icons: Vec<(Vec<u8>, u32)>,
    on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
) -> Result<Tray, String> {
    if icons.is_empty() {
        return Err("no icon for the tray".into());
    }
    imp::create(icons, on)
}

#[cfg(windows)]
mod imp {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
    use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

    use std::sync::{Mutex, Once};

    use tray_icon::menu::MenuId;

    use super::{LABELS, Routes, SUBMENUS_BEFORE, TOOLTIP, Tray, TrayCommand, submenus};
    use chonkgene_core::i18n::tr;

    static ROUTES: Mutex<Routes<MenuId>> = Mutex::new(Routes::new());
    static HANDLERS: Once = Once::new();

    pub fn create(
        icons: Vec<(Vec<u8>, u32)>,
        on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
    ) -> Result<Tray, String> {
        let menu = Menu::new();
        let mut ids = Vec::new();
        for (i, (label, command)) in LABELS.iter().enumerate() {
            if i == SUBMENUS_BEFORE {
                for (title, entries) in submenus() {
                    let sub = Submenu::new(tr(title), true);
                    for (label, command) in entries {
                        let item = MenuItem::new(tr(label), true, None);
                        ids.push((item.id().clone(), command));
                        sub.append(&item).map_err(|e| e.to_string())?;
                    }
                    menu.append(&sub).map_err(|e| e.to_string())?;
                }
            }
            if i == LABELS.len() - 1 {
                menu.append(&PredefinedMenuItem::separator())
                    .map_err(|e| e.to_string())?;
            }
            let item = MenuItem::new(tr(label), true, None);
            ids.push((item.id().clone(), *command));
            menu.append(&item).map_err(|e| e.to_string())?;
        }
        let (rgba, size) = icons.into_iter().next().ok_or("no icon for the tray")?;
        let icon = Icon::from_rgba(rgba, size, size).map_err(|e| e.to_string())?;
        let tray = TrayIconBuilder::new()
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .with_tooltip(tr(TOOLTIP))
            .with_icon(icon)
            .build()
            .map_err(|e| e.to_string())?;
        if let Ok(mut routes) = ROUTES.lock() {
            routes.set(ids, on);
        }
        // Set once for good (see `Routes`); they read whatever menu is up.
        HANDLERS.call_once(|| {
            MenuEvent::set_event_handler(Some(|e: MenuEvent| {
                if let Ok(routes) = ROUTES.lock() {
                    routes.chosen(&e.id);
                }
            }));
            TrayIconEvent::set_event_handler(Some(|e: TrayIconEvent| {
                if let TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                } = e
                    && let Ok(routes) = ROUTES.lock()
                {
                    routes.send(TrayCommand::Toggle);
                }
            }));
        });
        Ok(Tray { _icon: tray })
    }
}

#[cfg(target_os = "linux")]
pub struct Sni {
    icons: Vec<ksni::Icon>,
    on: Box<dyn Fn(TrayCommand) + Send + Sync>,
}

use chonkgene_core::i18n::mark;
#[cfg(target_os = "linux")]
use chonkgene_core::i18n::tr;

#[cfg(target_os = "linux")]
impl ksni::Tray for Sni {
    fn id(&self) -> String {
        "chonkgene".into()
    }

    fn title(&self) -> String {
        tr(TOOLTIP).into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        (self.on)(TrayCommand::Toggle);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        let mut items = Vec::new();
        for (i, (label, command)) in LABELS.iter().enumerate() {
            if i == SUBMENUS_BEFORE {
                for (title, entries) in submenus() {
                    let submenu = entries
                        .into_iter()
                        .map(|(label, command)| {
                            ksni::menu::StandardItem {
                                label: tr(label).into(),
                                activate: Box::new(move |this: &mut Self| (this.on)(command)),
                                ..Default::default()
                            }
                            .into()
                        })
                        .collect();
                    items.push(
                        ksni::menu::SubMenu {
                            label: tr(title).into(),
                            submenu,
                            ..Default::default()
                        }
                        .into(),
                    );
                }
            }
            if i == LABELS.len() - 1 {
                items.push(ksni::MenuItem::Separator);
            }
            let command = *command;
            items.push(
                ksni::menu::StandardItem {
                    label: tr(label).into(),
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
        icons: Vec<(Vec<u8>, u32)>,
        on: impl Fn(TrayCommand) + Send + Sync + Clone + 'static,
    ) -> Result<Tray, String> {
        // StatusNotifierItem wants ARGB32 in network byte order.
        let icons = icons
            .into_iter()
            .map(|(rgba, size)| {
                let (px, _) = rgba.as_chunks::<4>();
                ksni::Icon {
                    width: size as i32,
                    height: size as i32,
                    data: px.iter().flat_map(|p| [p[3], p[0], p[1], p[2]]).collect(),
                }
            })
            .collect();
        let sni = Sni {
            icons,
            on: Box::new(on),
        };
        let handle = sni.spawn().map_err(|e| e.to_string())?;
        Ok(Tray { _handle: handle })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{Routes, TrayCommand};

    /// The tray is made again when the language changes, with new item
    /// ids; the one handler (set once) must follow the new menu.
    #[test]
    fn a_remade_tray_menu_still_sends_its_commands() {
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = |sent: &Arc<Mutex<Vec<TrayCommand>>>| {
            let sent = sent.clone();
            move |c| sent.lock().unwrap().push(c)
        };
        let mut routes: Routes<u32> = Routes::new();
        routes.set(
            vec![(1, TrayCommand::Ball), (2, TrayCommand::Quit)],
            log(&sent),
        );
        assert!(routes.chosen(&1));
        // Remade in another language: new ids for the same commands.
        routes.set(
            vec![(11, TrayCommand::Ball), (12, TrayCommand::Settings)],
            log(&sent),
        );
        assert!(!routes.chosen(&1), "an id of the old menu");
        assert!(routes.chosen(&12));
        routes.send(TrayCommand::Toggle);
        assert_eq!(
            *sent.lock().unwrap(),
            [
                TrayCommand::Ball,
                TrayCommand::Settings,
                TrayCommand::Toggle
            ]
        );
    }
}
