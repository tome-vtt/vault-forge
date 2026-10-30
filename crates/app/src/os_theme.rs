//! The OS theme where the window layer cannot report it. On Windows and macOS the window reports
//! it itself, so this sends nothing. On Linux it reads the desktop portal's colour scheme on a
//! worker thread and sends each change.
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{Context, Theme};

pub fn watch(ctx: Context) -> Receiver<Theme> {
    let (tx, rx) = channel();
    #[cfg(target_os = "linux")]
    std::thread::spawn(move || {
        // No portal (a bare window manager, say): the app keeps egui's fallback theme.
        let _ = portal::watch(|theme| {
            if tx.send(theme).is_ok() {
                ctx.request_repaint();
            }
        });
    });
    #[cfg(not(target_os = "linux"))]
    let _ = (tx, ctx);
    rx
}

#[cfg(target_os = "linux")]
mod portal {
    use eframe::egui::Theme;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::OwnedValue;

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    /// The portal's colour scheme: 1 prefers dark; 0 (no preference) and 2 show light.
    pub fn theme(scheme: u32) -> Theme {
        if scheme == 1 {
            Theme::Dark
        } else {
            Theme::Light
        }
    }

    /// Sends the current theme, then each change until the session bus goes away.
    pub fn watch(send: impl Fn(Theme)) -> zbus::Result<()> {
        let bus = Connection::session()?;
        let settings = Proxy::new(
            &bus,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        // Subscribe first, so a change while reading the current value is not missed.
        let changes = settings.receive_signal("SettingChanged")?;
        let scheme: OwnedValue = settings.call("ReadOne", &(NAMESPACE, KEY))?;
        send(theme(u32::try_from(scheme)?));
        for change in changes {
            let (namespace, key, scheme): (String, String, OwnedValue) =
                change.body().deserialize()?;
            if namespace == NAMESPACE && key == KEY {
                send(theme(u32::try_from(scheme)?));
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_a_dark_preference_shows_dark() {
            assert_eq!(theme(0), Theme::Light);
            assert_eq!(theme(1), Theme::Dark);
            assert_eq!(theme(2), Theme::Light);
        }
    }
}
