use iced::{
    theme::{Palette, Theme},
    Color,
};

const SAMAKU_BACKGROUND: Color = Color::from_rgb(
    0x11 as f32 / 255.0,
    0x11 as f32 / 255.0,
    0x11 as f32 / 255.0,
);

const SAMAKU_PRIMARY: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0x11 as f32 / 255.0,
    0x11 as f32 / 255.0,
);

const SAMAKU_SUCCESS: Color = Color::from_rgb(
    0x11 as f32 / 255.0,
    0xff as f32 / 255.0,
    0x11 as f32 / 255.0,
);

const SAMAKU_DESTRUCTIVE: Color = Color::from_rgb(
    0x11 as f32 / 255.0,
    0x11 as f32 / 255.0,
    0xff as f32 / 255.0,
);

const SAMAKU_TEXT: Color = Color::from_rgb(
    0xaa as f32 / 255.0,
    0x11 as f32 / 255.0,
    0xaa as f32 / 255.0,
);

pub fn samaku_theme() -> Theme {
    return Theme::custom(Palette {
        background: SAMAKU_BACKGROUND,
        text: SAMAKU_TEXT,
        primary: SAMAKU_PRIMARY,
        danger: SAMAKU_DESTRUCTIVE,
        success: SAMAKU_SUCCESS,
    });
}
