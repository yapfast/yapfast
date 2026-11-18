//! Whether the desktop gives windows a title bar, which decides whether
//! Yapfast draws its own until the person chooses in Appearance.
//!
//! A desktop that gives its windows title bars, as KDE and GNOME do, draws
//! one on Yapfast's too unless Yapfast draws its own. A tiling compositor such
//! as Hyprland or sway draws none, and there Yapfast's would be in the way.
//! The desktop doesn't say which it does: KWin and Hyprland both answer
//! xdg-decoration with server-side. So `XDG_CURRENT_DESKTOP` decides.

/// Desktops whose windows have title bars, as `XDG_CURRENT_DESKTOP` names them.
const FRAMED: [&str; 14] = [
    "KDE",
    "GNOME",
    "Unity",
    "Cinnamon",
    "X-Cinnamon",
    "XFCE",
    "MATE",
    "LXQt",
    "LXDE",
    "Budgie",
    "Pantheon",
    "Deepin",
    "COSMIC",
    "Enlightenment",
];

/// Whether any of the desktops in a colon-separated `XDG_CURRENT_DESKTOP`,
/// such as "ubuntu:GNOME", gives its windows title bars.
pub fn framed(desktops: &str) -> bool {
    desktops.split(':').any(|desktop| {
        FRAMED
            .iter()
            .any(|framed| desktop.eq_ignore_ascii_case(framed))
    })
}

/// Whether this desktop, as `XDG_CURRENT_DESKTOP` names it, frames windows.
pub fn desktop_frames() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP").is_ok_and(|desktops| framed(&desktops))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framed_desktops_by_name() {
        assert!(framed("KDE"));
        assert!(framed("ubuntu:GNOME"));
        assert!(framed("X-Cinnamon"));
        assert!(!framed("Hyprland"));
        assert!(!framed("sway"));
        assert!(!framed("niri"));
        assert!(!framed(""));
    }
}
