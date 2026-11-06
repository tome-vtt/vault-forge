//! The OS theme where the window layer cannot report it. On Windows and macOS the window reports
//! it itself, so this sends nothing. On Linux it reads the desktop portal's colour scheme on a
//! worker thread and sends each change.
//!
//! It also reads whether the OS asks for reduced motion, which no window layer reports: Windows'
//! Show animations, macOS's Reduce motion, and on Linux the portal's GNOME animations switch.
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{Context, Theme};

use crate::theme::set_reduce_motion;

/// Starts watching. The first reading, if it comes quickly, is applied before this returns, so
/// the first frame is already in the OS theme.
pub fn watch(ctx: Context) -> Receiver<Theme> {
    let (tx, rx) = channel();
    #[cfg(target_os = "linux")]
    {
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            // No portal (a bare window manager, say): the app keeps egui's fallback theme.
            let _ = portal::watch(
                |theme| {
                    if tx.send(theme).is_ok() {
                        ctx.request_repaint();
                    }
                },
                |reduce| {
                    set_reduce_motion(&ctx, reduce);
                    ctx.request_repaint();
                },
            );
        });
    }
    #[cfg(not(target_os = "linux"))]
    {
        drop(tx);
        // ponytail: read once at start; a change takes effect when the app next opens.
        set_reduce_motion(&ctx, reduce_motion());
    }
    if let Ok(theme) = rx.recv_timeout(std::time::Duration::from_millis(250)) {
        ctx.options_mut(|o| o.fallback_theme = theme);
    }
    rx
}

/// Windows' Settings > Accessibility > Visual effects > Animation effects, off.
#[cfg(windows)]
fn reduce_motion() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut on: windows_sys::core::BOOL = 1;
    // SAFETY: this action writes one BOOL through the pointer, which points at `on`.
    let read = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut on).cast(), 0)
    };
    read != 0 && on == 0
}

/// macOS's System Settings > Accessibility > Display > Reduce motion.
#[cfg(target_os = "macos")]
fn reduce_motion() -> bool {
    objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
fn reduce_motion() -> bool {
    false
}

#[cfg(target_os = "linux")]
mod portal {
    use eframe::egui::Theme;
    use zbus::blocking::{Connection, Proxy};
    use zbus::zvariant::{OwnedValue, Value};

    const NAMESPACE: &str = "org.freedesktop.appearance";
    const KEY: &str = "color-scheme";
    // ponytail: GNOME's switch only; KDE's animation speed would need its own key.
    const MOTION_NAMESPACE: &str = "org.gnome.desktop.interface";
    const MOTION_KEY: &str = "enable-animations";

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

    /// GNOME's animations switch: off asks for reduced motion.
    pub fn reduce_motion(animations: &Value) -> Option<bool> {
        match animations {
            Value::Bool(on) => Some(!on),
            Value::Value(inner) => reduce_motion(inner),
            _ => None,
        }
    }

    /// Sends the current theme and reduced motion, then each change until the session bus goes
    /// away.
    pub fn watch(send: impl Fn(Theme), send_motion: impl Fn(bool)) -> zbus::Result<()> {
        let bus = Connection::session()?;
        let settings = Proxy::new(
            &bus,
            "org.freedesktop.portal.Desktop",
            "/org/freedesktop/portal/desktop",
            "org.freedesktop.portal.Settings",
        )?;
        // Subscribe first, so a change while reading the current value is not missed.
        let changes = settings.receive_signal("SettingChanged")?;
        // Desktops without GNOME's key leave the track sliding.
        if let Ok(animations) = settings
            .call::<_, _, OwnedValue>("ReadOne", &(MOTION_NAMESPACE, MOTION_KEY))
            .or_else(|_| settings.call("Read", &(MOTION_NAMESPACE, MOTION_KEY)))
            && let Some(reduce) = reduce_motion(&animations)
        {
            send_motion(reduce);
        }
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
            if namespace == MOTION_NAMESPACE
                && key == MOTION_KEY
                && let Some(reduce) = reduce_motion(&scheme)
            {
                send_motion(reduce);
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
        fn animations_switched_off_ask_for_reduced_motion() {
            assert_eq!(reduce_motion(&Value::Bool(false)), Some(true));
            assert_eq!(reduce_motion(&Value::Bool(true)), Some(false));
            let wrapped = Value::Value(Box::new(Value::Bool(false)));
            assert_eq!(reduce_motion(&wrapped), Some(true));
            assert_eq!(reduce_motion(&Value::U32(0)), None);
        }

        #[test]
        fn reads_the_older_portal_s_wrapped_value() {
            let wrapped = Value::Value(Box::new(Value::U32(1)));
            assert_eq!(theme(&wrapped), Some(Theme::Dark));
        }
    }
}
