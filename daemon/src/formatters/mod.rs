#![allow(dead_code)]

pub mod waybar;
pub mod terminal;
pub mod gnome;

pub use waybar::WaybarFrontendFormatter;
pub use terminal::TerminalFrontendFormatter;
pub use gnome::GnomeFrontendFormatter;