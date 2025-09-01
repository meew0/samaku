#![expect(
    clippy::cast_precision_loss,
    reason = "precision really does not matter in this case"
)]

use iced::widget::container;
use iced::{
    theme::{Palette, Theme},
    Color,
};

// https://coolors.co/0f1a19-f0f7ee-ffb60a-06b153-f5004e

pub const SAMAKU_BACKGROUND: Color = Color::from_rgb(
    0x0f as f32 / 255.0,
    0x1a as f32 / 255.0,
    0x19 as f32 / 255.0,
);

pub const SAMAKU_TEXT: Color = Color::from_rgb(
    0xf0 as f32 / 255.0,
    0xf7 as f32 / 255.0,
    0xee as f32 / 255.0,
);

pub const SAMAKU_TEXT_WEAK: Color = Color::from_rgb(
    0x97 as f32 / 255.0,
    0xa0 as f32 / 255.0,
    0x95 as f32 / 255.0,
);

pub const SAMAKU_PRIMARY_RED: u8 = 0xff;
pub const SAMAKU_PRIMARY_GREEN: u8 = 0xb6;
pub const SAMAKU_PRIMARY_BLUE: u8 = 0x0a;

pub const SAMAKU_PRIMARY: Color = Color::from_rgb(
    SAMAKU_PRIMARY_RED as f32 / 255.0,
    SAMAKU_PRIMARY_GREEN as f32 / 255.0,
    SAMAKU_PRIMARY_BLUE as f32 / 255.0,
);

pub const SAMAKU_INACTIVE: Color = Color::from_rgb(
    0x66 as f32 / 255.0,
    0x75 as f32 / 255.0,
    0x74 as f32 / 255.0,
);

pub const SAMAKU_SUCCESS: Color = Color::from_rgb(
    0x06 as f32 / 255.0,
    0xb1 as f32 / 255.0,
    0x53 as f32 / 255.0,
);

pub const SAMAKU_DESTRUCTIVE: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0x00 as f32 / 255.0,
    0x4e as f32 / 255.0,
);

#[must_use]
pub fn samaku_theme() -> Theme {
    Theme::custom(
        "samaku".to_owned(),
        Palette {
            background: SAMAKU_BACKGROUND,
            text: SAMAKU_TEXT,
            primary: SAMAKU_PRIMARY,
            danger: SAMAKU_DESTRUCTIVE,
            success: SAMAKU_SUCCESS,
        },
    )
}

#[must_use]
pub fn title_bar_active(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        text_color: Some(palette.background.weak.text),
        background: Some(palette.background.weak.color.into()),
        border: iced::Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: [0.0_f32; 4].into(),
        },
        ..Default::default()
    }
}

#[must_use]
pub fn title_bar_focused(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        text_color: Some(palette.primary.strong.text),
        background: Some(palette.primary.strong.color.into()),
        ..Default::default()
    }
}

#[must_use]
pub fn pane_active(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        background: None,
        border: iced::Border {
            width: 1.0,
            color: palette.background.weak.color,
            radius: [0.0_f32; 4].into(),
        },
        ..Default::default()
    }
}

#[must_use]
pub fn pane_focused(theme: &Theme) -> container::Appearance {
    let palette = theme.extended_palette();

    container::Appearance {
        background: None,
        border: iced::Border {
            width: 2.0,
            color: palette.background.strong.color,
            radius: [0.0_f32; 4].into(),
        },
        ..Default::default()
    }
}
