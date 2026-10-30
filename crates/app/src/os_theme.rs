//! The OS theme where the window layer cannot report it. On Windows and macOS the window reports
//! it itself, so this sends nothing. On Linux it reads the desktop portal's colour scheme on a
//! worker thread and sends each change.
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{Context, Theme};

/// Starts watching. The first reading, if it comes quickly, is applied before this returns, so
/// the first frame is already in the OS theme.
pub fn watch(ctx: Context) -> Receiver<Theme> {
    let (tx, rx) = channel();
    #[cfg(target_os = "linux")]
    {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // No portal (a bare window manager, say): the app keeps egui's fallback theme.
            let _ = portal::watch(|theme| {
                if tx.send(theme).is_ok() {
                    ctx.request_repaint();
                }
            });
        });
    }
    #[cfg(not(target_os = "linux"))]
    drop(tx);
    if let Ok(theme) = rx.recv_timeout(std::time::Duration::from_millis(250)) {
        ctx.options_mut(|o| o.fallback_theme = theme);
    }
    rx
}

#[cfg(target_os = "linux")]
mod portal {
    use eframe::egui::Theme;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";

    /// The portal's colour scheme: 1 prefers dark; 0 (no preference) and 2 show light.
    pub fn theme(scheme: &Value) -> Option<Theme> {
        match scheme {
            Value::U32(1) => Some(Theme::Dark),
            Value::U32(_) => Some(Theme::Light),
            // Portals older than `ReadOne` wrap the value once more.
            Value::Value(inner) => theme(inner),
            _ => None,
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
        let scheme: OwnedValue = settings
            .call("ReadOne", &(NAMESPACE, KEY))
            .or_else(|_| settings.call("Read", &(NAMESPACE, KEY)))?;
        if let Some(theme) = theme(&scheme) {
            send(theme);
        }
        for change in changes {
            let Ok((namespace, key, scheme)) =
                change.body().deserialize::<(String, String, OwnedValue)>()
            else {
                continue;
            };
            if namespace == NAMESPACE
                && key == KEY
                && let Some(theme) = theme(&scheme)
            {
                send(theme);
            }
        }
        Ok(())
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn only_a_dark_preference_shows_dark() {
            assert_eq!(theme(&Value::U32(0)), Some(Theme::Light));
            assert_eq!(theme(&Value::U32(1)), Some(Theme::Dark));
            assert_eq!(theme(&Value::U32(2)), Some(Theme::Light));
        }

        #[test]
        fn reads_the_older_portal_s_wrapped_value() {
            let wrapped = Value::Value(Box::new(Value::U32(1)));
            assert_eq!(theme(&wrapped), Some(Theme::Dark));
        }
    }
}
