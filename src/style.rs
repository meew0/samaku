use iced::widget::container;
use iced::{
    theme::{Palette, Theme},
    Color,
};

// https://coolors.co/0f1a19-f0f7ee-ffb60a-06b153-f5004e

const SAMAKU_BACKGROUND: Color = Color::from_rgb(
    0x0f as f32 / 255.0,
    0x1a as f32 / 255.0,
    0x19 as f32 / 255.0,
);

const SAMAKU_TEXT: Color = Color::from_rgb(
    0xf0 as f32 / 255.0,
    0xf7 as f32 / 255.0,
    0xee as f32 / 255.0,
);

const SAMAKU_PRIMARY: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xb6 as f32 / 255.0,
    0x0a as f32 / 255.0,
);

const SAMAKU_SUCCESS: Color = Color::from_rgb(
    0x06 as f32 / 255.0,
    0xb1 as f32 / 255.0,
    0x53 as f32 / 255.0,
);

const SAMAKU_DESTRUCTIVE: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0x00 as f32 / 255.0,
    0x4e as f32 / 255.0,
);

pub fn samaku_theme() -> Theme {
    Theme::custom(Palette {
        background: SAMAKU_BACKGROUND,
        text: SAMAKU_TEXT,
        primary: SAMAKU_PRIMARY,
        danger: SAMAKU_DESTRUCTIVE,
        success: SAMAKU_SUCCESS,
    })
}

pub fn title_bar_active(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        text_color: Some(palette.background.weak.text),
        background: Some(palette.background.weak.color.into()),
        border_width: 1.0,
        border_color: palette.background.weak.color,
        ..Default::default()
    }
}

pub fn title_bar_focused(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        text_color: Some(palette.primary.strong.text),
        background: Some(palette.primary.strong.color.into()),
        ..Default::default()
    }
}

pub fn pane_active(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        background: None,
        border_width: 1.0,
        border_color: palette.background.weak.color,
        ..Default::default()
    }
}

pub fn pane_focused(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        background: None,
        border_width: 2.0,
        border_color: palette.primary.strong.color,
        ..Default::default()
    }
}
