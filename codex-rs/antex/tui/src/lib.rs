mod appearance;
mod custom_terminal;
mod editor_types;
mod insert_history;
mod key_hint;
mod keymap;
mod keymap_config;
mod line_truncation;
mod mascot;
mod mascot_palette;
mod render;
mod terminal_hyperlinks;
mod terminal_palette;
#[cfg(test)]
mod test_backend;
mod textarea;
mod tui;
mod vim_search;
mod width;
mod wrapping;

pub use appearance::StartupMascotSkin;
