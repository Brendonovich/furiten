//! Who this build is: Furiten, or Furiten Dev (`FURITEN_CHANNEL=dev` at build time). The dev build keeps its own
//! preferences and runs beside Furiten, so both can be open at once with libraries of their own.

/// The name menus show and the bundle is named.
pub fn name() -> &'static str {
    if furiten_core::is_dev_build() { "Furiten Dev" } else { "Furiten" }
}

/// Where the app keeps its preferences (`rune::prefs`).
pub fn prefs_id() -> &'static str {
    if furiten_core::is_dev_build() { PREFS_ID_DEV } else { PREFS_ID }
}

/// The icon `rune::set_app_icon` shows before a bundle's icon applies, from `scripts/icons.sh`.
pub fn icon() -> &'static [u8] {
    if furiten_core::is_dev_build() { include_bytes!("../assets/icon-dev.png") } else { include_bytes!("../assets/icon.png") }
}

const PREFS_ID: &str = "furiten-desktop";
const PREFS_ID_DEV: &str = "furiten-desktop-dev";
