use std::marker::PhantomData;

use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard::{self, key};
use iced::{Background, Border, Color, Element, Event, Length, Point, Rectangle, Shadow, Size};

const BUTTON_WIDTH: f32 = 20.0;
const DRAG_THRESHOLD: f32 = 3.0;
const DEFAULT_HEIGHT: f32 = 24.0;

/// A Blender-style numeric drag field.
///
/// Shows the current value; arrows appear on hover to step the value; click-drag in
/// the centre scrubs proportionally; click-without-dragging opens an editable text
/// field.  All interaction state is stored in the iced widget tree — the caller only
/// needs to supply the current `f64` value.
pub struct NumberDragger<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    Theme: Catalog,
{
    value: f64,
    min: f64,
    max: f64,
    step: f64,
    drag_speed: f64,
    decimals: u32,
    on_change: Box<dyn Fn(f64) -> Message + 'a>,
    width: Length,
    height: f32,
    class: <Theme as Catalog>::Class<'a>,
    _phantom: PhantomData<Renderer>,
}

impl<'a, Message, Theme, Renderer> NumberDragger<'a, Message, Theme, Renderer>
where
    Theme: Catalog,
{
    pub fn new<F: Fn(f64) -> Message + 'a>(
        value: f64,
        bounds: std::ops::RangeInclusive<f64>,
        on_change: F,
    ) -> Self {
        let (min, max) = bounds.into_inner();
        Self {
            value,
            min,
            max,
            step: 1.0,
            drag_speed: 1.0,
            decimals: 2,
            on_change: Box::new(on_change),
            width: Length::Fill,
            height: DEFAULT_HEIGHT,
            class: <Theme as Catalog>::default(),
            _phantom: PhantomData,
        }
    }

    #[must_use]
    pub fn step(mut self, step: f64) -> Self {
        self.step = step;
        self
    }

    #[must_use]
    pub fn drag_speed(mut self, speed: f64) -> Self {
        self.drag_speed = speed;
        self
    }

    #[must_use]
    pub fn decimals(mut self, decimals: u32) -> Self {
        self.decimals = decimals;
        self
    }

    #[must_use]
    pub fn width<W: Into<Length>>(mut self, width: W) -> Self {
        self.width = width.into();
        self
    }
}

// ─── Internal widget-tree state ───────────────────────────────────────────────

struct State {
    mode: Mode,
    is_hovered: bool,
}

impl Default for State {
    fn default() -> Self {
        Self {
            mode: Mode::Idle,
            is_hovered: false,
        }
    }
}

enum Mode {
    Idle,
    /// Mouse pressed; not yet moved far enough to commit to a drag vs. a plain click.
    Pending {
        start_x: f32,
        start_value: f64,
    },
    Dragging {
        last_x: f32,
    },
    Editing {
        text: String,
    },
}

// ─── Theme / Catalog ─────────────────────────────────────────────────────────

pub struct Style {
    pub background: Background,
    pub border: Border,
    pub text_color: Color,
    pub icon_color: Color,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Active,
    Hovered,
    Dragging,
    Editing,
}

pub type StyleFn<'a, Theme> = Box<dyn Fn(&Theme, Status) -> Style + 'a>;

pub trait Catalog {
    type Class<'a>;

    fn default<'a>() -> Self::Class<'a>;

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style;
}

impl Catalog for iced::Theme {
    type Class<'a> = StyleFn<'a, iced::Theme>;

    fn default<'a>() -> Self::Class<'a> {
        Box::new(default_style)
    }

    fn style(&self, class: &Self::Class<'_>, status: Status) -> Style {
        class(self, status)
    }
}

fn default_style(theme: &iced::Theme, status: Status) -> Style {
    let palette = theme.extended_palette();
    match status {
        Status::Active => Style {
            background: Background::Color(palette.background.weak.color),
            border: Border::default()
                .width(1.0)
                .color(palette.background.strong.color),
            text_color: palette.background.base.text,
            icon_color: palette.background.strong.color,
        },
        Status::Hovered => Style {
            background: Background::Color(palette.background.base.color),
            border: Border::default()
                .width(1.0)
                .color(palette.primary.base.color),
            text_color: palette.background.base.text,
            icon_color: palette.primary.base.color,
        },
        Status::Dragging => Style {
            background: Background::Color(palette.primary.weak.color),
            border: Border::default()
                .width(1.0)
                .color(palette.primary.base.color),
            text_color: palette.primary.weak.text,
            icon_color: palette.primary.base.color,
        },
        Status::Editing => Style {
            background: Background::Color(palette.background.base.color),
            border: Border::default()
                .width(1.5)
                .color(palette.primary.strong.color),
            text_color: palette.background.base.text,
            icon_color: palette.primary.strong.color,
        },
    }
}

// ─── Private helpers ─────────────────────────────────────────────────────────

fn clamp_and_round(value: f64, min: f64, max: f64, decimals: u32) -> f64 {
    let clamped = value.clamp(min, max);
    #[expect(
        clippy::cast_possible_wrap,
        reason = "decimals will not exceed i32::MAX"
    )]
    let factor = 10_f64.powi(decimals as i32);
    (clamped * factor).round() / factor
}

fn format_value(value: f64, decimals: u32) -> String {
    format!("{:.prec$}", value, prec = decimals as usize)
}

fn make_text_primitive<Font: Copy>(
    content: String,
    bounds: Size,
    size: iced::Pixels,
    font: Font,
) -> text::Text<String, Font> {
    text::Text {
        content,
        bounds,
        size,
        line_height: text::LineHeight::default(),
        font,
        align_x: text::Alignment::Center,
        align_y: iced::alignment::Vertical::Center,
        shaping: text::Shaping::Basic,
        wrapping: text::Wrapping::None,
    }
}

// ─── Widget implementation ────────────────────────────────────────────────────

impl<Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for NumberDragger<'_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: text::Renderer,
    Theme: Catalog,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<State>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(State::default())
    }

    fn children(&self) -> Vec<widget::Tree> {
        vec![]
    }

    fn diff(&self, _tree: &mut widget::Tree) {}

    fn size(&self) -> Size<Length> {
        Size::new(self.width, Length::Fixed(self.height))
    }

    fn layout(
        &mut self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::Node::new(limits.resolve(self.width, Length::Fixed(self.height), Size::ZERO))
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        let status = if matches!(state.mode, Mode::Editing { .. }) {
            Status::Editing
        } else if matches!(state.mode, Mode::Dragging { .. } | Mode::Pending { .. }) {
            Status::Dragging
        } else if state.is_hovered {
            Status::Hovered
        } else {
            Status::Active
        };

        let style = theme.style(&self.class, status);

        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: style.border,
                shadow: Shadow::default(),
                snap: true,
            },
            style.background,
        );

        let text_size = renderer.default_size();
        let font = renderer.default_font();
        let center = bounds.center();

        if let Mode::Editing { ref text } = state.mode {
            renderer.fill_text(
                make_text_primitive(format!("{text}|"), bounds.size(), text_size, font),
                center,
                style.text_color,
                bounds,
            );
        } else {
            let show_arrows = state.is_hovered
                && !matches!(state.mode, Mode::Dragging { .. } | Mode::Pending { .. });

            renderer.fill_text(
                make_text_primitive(
                    format_value(self.value, self.decimals),
                    bounds.size(),
                    text_size,
                    font,
                ),
                center,
                style.text_color,
                bounds,
            );

            if show_arrows {
                let arrow_size = Size::new(BUTTON_WIDTH, bounds.height);
                renderer.fill_text(
                    make_text_primitive("<".to_owned(), arrow_size, text_size, font),
                    Point::new(bounds.x + BUTTON_WIDTH / 2.0, center.y),
                    style.icon_color,
                    bounds,
                );
                renderer.fill_text(
                    make_text_primitive(">".to_owned(), arrow_size, text_size, font),
                    Point::new(bounds.x + bounds.width - BUTTON_WIDTH / 2.0, center.y),
                    style.icon_color,
                    bounds,
                );
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let state = tree.state.downcast_ref::<State>();
        let bounds = layout.bounds();

        if matches!(state.mode, Mode::Editing { .. }) {
            mouse::Interaction::Text
        } else if matches!(state.mode, Mode::Dragging { .. } | Mode::Pending { .. }) {
            mouse::Interaction::ResizingHorizontally
        } else if let Some(pos) = cursor.position_in(bounds) {
            if pos.x < BUTTON_WIDTH || pos.x > bounds.width - BUTTON_WIDTH {
                mouse::Interaction::Pointer
            } else {
                mouse::Interaction::ResizingHorizontally
            }
        } else {
            mouse::Interaction::default()
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "state machine for all interaction modes; decomposing further adds indirection without clarity"
    )]
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) {
        let state = tree.state.downcast_mut::<State>();
        let bounds = layout.bounds();

        match *event {
            Event::Mouse(mouse::Event::CursorMoved { position }) => {
                let is_over = bounds.contains(position);
                if is_over != state.is_hovered {
                    state.is_hovered = is_over;
                    shell.request_redraw();
                }

                // Compute whether we should leave Pending and enter Dragging.
                // Separating the read from the write avoids borrow conflicts.
                let pending_update = match &state.mode {
                    &Mode::Pending {
                        start_x,
                        start_value,
                    } => {
                        let dx = position.x - start_x;
                        (dx.abs() > DRAG_THRESHOLD).then(|| {
                            let new_value = clamp_and_round(
                                f64::from(dx).mul_add(self.drag_speed, start_value),
                                self.min,
                                self.max,
                                self.decimals,
                            );
                            (position.x, new_value)
                        })
                    }
                    _ => None,
                };

                if let Some((new_last_x, new_value)) = pending_update {
                    state.mode = Mode::Dragging { last_x: new_last_x };
                    if (new_value - self.value).abs() > f64::EPSILON {
                        shell.publish((self.on_change)(new_value));
                    }
                    shell.request_redraw();
                }
                if pending_update.is_none()
                    && let Mode::Dragging { ref mut last_x } = state.mode
                {
                    let dx = position.x - *last_x;
                    *last_x = position.x;
                    if dx != 0.0 {
                        let new_value = clamp_and_round(
                            f64::from(dx).mul_add(self.drag_speed, self.value),
                            self.min,
                            self.max,
                            self.decimals,
                        );
                        if (new_value - self.value).abs() > f64::EPSILON {
                            shell.publish((self.on_change)(new_value));
                        }
                    }
                }
            }

            Event::Mouse(mouse::Event::CursorLeft) if state.is_hovered => {
                state.is_hovered = false;
                shell.request_redraw();
            }

            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let inside = cursor.position_in(bounds);
                if let Some(local_pos) = inside
                    && !matches!(state.mode, Mode::Editing { .. })
                {
                    if local_pos.x < BUTTON_WIDTH {
                        let new_value = clamp_and_round(
                            self.value - self.step,
                            self.min,
                            self.max,
                            self.decimals,
                        );
                        shell.publish((self.on_change)(new_value));
                    } else if local_pos.x > bounds.width - BUTTON_WIDTH {
                        let new_value = clamp_and_round(
                            self.value + self.step,
                            self.min,
                            self.max,
                            self.decimals,
                        );
                        shell.publish((self.on_change)(new_value));
                    } else {
                        let abs_pos = cursor.position().unwrap_or(Point::ORIGIN);
                        state.mode = Mode::Pending {
                            start_x: abs_pos.x,
                            start_value: self.value,
                        };
                    }
                    shell.capture_event();
                    shell.request_redraw();
                }
                if inside.is_none()
                    && let Mode::Editing { ref text } = state.mode
                {
                    // Click outside while editing: commit the entered text
                    let new_value = text
                        .parse::<f64>()
                        .ok()
                        .map(|val| clamp_and_round(val, self.min, self.max, self.decimals));
                    if let Some(new_val) = new_value
                        && (new_val - self.value).abs() > f64::EPSILON
                    {
                        shell.publish((self.on_change)(new_val));
                    }
                    state.mode = Mode::Idle;
                    shell.request_redraw();
                }
            }

            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => match state.mode {
                Mode::Pending { .. } => {
                    state.mode = Mode::Editing {
                        text: format_value(self.value, self.decimals),
                    };
                    shell.capture_event();
                    shell.request_redraw();
                }
                Mode::Dragging { .. } => {
                    state.mode = Mode::Idle;
                    shell.capture_event();
                    shell.request_redraw();
                }
                Mode::Idle | Mode::Editing { .. } => {}
            },

            Event::Keyboard(keyboard::Event::KeyPressed {
                ref key, ref text, ..
            }) => {
                if let Mode::Editing {
                    text: ref mut edit_text,
                } = state.mode
                {
                    shell.capture_event();
                    match *key {
                        keyboard::Key::Named(key::Named::Enter) => {
                            let new_value = edit_text
                                .parse::<f64>()
                                .ok()
                                .map(|val| clamp_and_round(val, self.min, self.max, self.decimals));
                            if let Some(new_val) = new_value
                                && (new_val - self.value).abs() > f64::EPSILON
                            {
                                shell.publish((self.on_change)(new_val));
                            }
                            state.mode = Mode::Idle;
                        }
                        keyboard::Key::Named(key::Named::Escape) => {
                            state.mode = Mode::Idle;
                        }
                        keyboard::Key::Named(key::Named::Backspace) => {
                            let new_len = edit_text
                                .char_indices()
                                .next_back()
                                .map_or(0, |(idx, _)| idx);
                            edit_text.truncate(new_len);
                        }
                        _ => {
                            if let Some(typed) = text.as_ref() {
                                for ch in typed.chars() {
                                    if ch.is_ascii_digit() || ch == '.' || ch == '-' {
                                        edit_text.push(ch);
                                    }
                                }
                            }
                        }
                    }
                    shell.request_redraw();
                }
            }

            _ => {}
        }
    }
}

impl<'a, Message, Theme, Renderer> From<NumberDragger<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(widget: NumberDragger<'a, Message, Theme, Renderer>) -> Self {
        Self::new(widget)
    }
}
