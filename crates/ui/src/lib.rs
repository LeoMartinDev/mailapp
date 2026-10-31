//! Desktop UI: design system, screens and their view-models (GPUI). Talks to the core only: sends actions, runs queries, listens to events.

mod demo;
mod design;

use std::borrow::Cow;

use gpui::{App, AppContext as _, Font, Pixels, TitlebarOptions, WindowOptions, font, px};
use theme::{LoadThemes, ThemeSettingsProvider, UiDensity};

/// Opens the main window and runs the UI until it is closed.
pub fn run() {
    gpui_platform::application().run(|cx: &mut App| {
        init(cx);
        let options = WindowOptions {
            titlebar: Some(TitlebarOptions {
                title: Some("mailapp".into()),
                ..Default::default()
            }),
            ..Default::default()
        };
        cx.open_window(options, |_, cx| cx.new(|_| demo::Demo))
            .expect("failed to open the main window");
        cx.on_window_closed(|cx, _| cx.quit()).detach();
        cx.activate(true);
    });
}

/// Outside Zed, its `ui` crate needs two globals: a theme, set by `theme::init` (Zed's built-in
/// dark theme), and a `ThemeSettingsProvider` (fonts, sizes, density), which Zed fills from its
/// settings store. Providing it ourselves keeps Zed's settings crates out.
fn init(cx: &mut App) {
    cx.text_system()
        .add_fonts(vec![
            Cow::Borrowed(include_bytes!("../assets/fonts/inter/Inter-Regular.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../assets/fonts/inter/Inter-Medium.ttf").as_slice()),
            Cow::Borrowed(include_bytes!("../assets/fonts/inter/Inter-SemiBold.ttf").as_slice()),
        ])
        .expect("failed to load the bundled Inter font");
    theme::init(LoadThemes::JustBase, cx);
    theme::set_theme_settings_provider(
        Box::new(Settings {
            ui_font: font("Inter"),
        }),
        cx,
    );
}

struct Settings {
    ui_font: Font,
}

impl ThemeSettingsProvider for Settings {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui_font
    }

    // Zed's code-editor font; we have no code editor.
    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui_font
    }

    fn ui_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }

    fn buffer_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }

    fn ui_density(&self, _: &App) -> UiDensity {
        UiDensity::Default
    }
}
