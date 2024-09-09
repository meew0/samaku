//! Code for showing toasts (dismissable notifications) over the UI.
//! Mostly copied from the iced `toast` example: https://github.com/iced-rs/iced/blob/bc9bb28b1ccd1248d63ccdfef2f57d7aa837abbb/examples/toast/src/main.rs

use std::fmt;
use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::event::{self, Event};
use iced::mouse;
use iced::window;
use iced::{Alignment, Element, Length, Point, Rectangle, Renderer, Size, Theme, Vector};

pub const DEFAULT_TIMEOUT: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Status {
    #[default]
    Primary,
    Secondary,
    Success,
    Danger,
}

impl Status {
    pub const ALL: &'static [Self] = &[Self::Primary, Self::Secondary, Self::Success, Self::Danger];
}

impl iced::widget::container::StyleSheet for Status {
    type Style = Theme;

    fn appearance(&self, style: &Theme) -> iced::widget::container::Appearance {
        let palette = style.extended_palette();

        let pair = match self {
            Status::Primary => palette.primary.weak,
            Status::Secondary => palette.secondary.weak,
            Status::Success => palette.success.weak,
            Status::Danger => palette.danger.weak,
        };

        iced::widget::container::Appearance {
            background: Some(pair.color.into()),
            text_color: pair.text.into(),
            ..Default::default()
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Status::Primary => "Primary",
            Status::Secondary => "Secondary",
            Status::Success => "Success",
            Status::Danger => "Danger",
        }
        .fmt(formatter)
    }
}

#[derive(Debug, Clone, Default)]
pub struct Toast {
    pub count: u32,
    pub title: String,
    pub body: String,
    pub status: Status,
}

impl Toast {
    #[must_use]
    pub fn new(status: Status, title: String, body: String) -> Self {
        Self {
            count: 1,
            title,
            body,
            status,
        }
    }
}

impl PartialEq for Toast {
    fn eq(&self, other: &Self) -> bool {
        // Ignore count
        self.title == other.title && self.body == other.body && self.status == other.status
    }
}

impl Eq for Toast {}

pub struct Manager<'a, Message, Theme> {
    content: Element<'a, Message, Theme>,
    toasts: Vec<Element<'a, Message, Theme>>,
    timeout_secs: u64,
    on_close: Box<dyn Fn(usize) -> Message + 'a>,
}

impl<'a, Message, Theme> Manager<'a, Message, Theme>
where
    Message: 'a + Clone,
    Theme: 'a
        + iced::widget::container::StyleSheet
        + iced::widget::text::StyleSheet
        + iced::widget::button::StyleSheet
        + iced::widget::rule::StyleSheet,
    <Theme as iced::widget::container::StyleSheet>::Style: From<iced::theme::Container>,
{
    pub fn new<E: Into<Element<'a, Message, Theme>>, F: Fn(usize) -> Message + 'a>(
        content: E,
        toasts: &'a [Toast],
        on_close: F,
    ) -> Self {
        let mut elements: Vec<Element<'a, Message, Theme>> = vec![];

        // In samaku, we want the toasts to appear at the bottom, so add a vertical space.
        elements.push(iced::widget::vertical_space().into());

        for (index, toast) in toasts.iter().enumerate() {
            let title_text = if toast.count == 1 {
                iced::widget::text(toast.title.as_str())
            } else {
                iced::widget::text(format!("({}x) {}", toast.count, toast.title))
            };

            elements.push(
                iced::widget::container(iced::widget::column![
                    iced::widget::container(
                        iced::widget::row![
                            title_text,
                            iced::widget::horizontal_space(),
                            iced::widget::button("X")
                                .on_press((on_close)(index))
                                .padding(3),
                        ]
                        .align_items(Alignment::Center)
                    )
                    .width(Length::Fill)
                    .padding(5)
                    .style(iced::theme::Container::Custom(Box::new(toast.status))),
                    iced::widget::horizontal_rule(1),
                    iced::widget::container(iced::widget::text(toast.body.as_str()))
                        .width(Length::Fill)
                        .padding(5)
                        .style(iced::theme::Container::Box),
                ])
                .max_width(200)
                .into(),
            );
        }

        Self {
            content: content.into(),
            toasts: elements,
            timeout_secs: DEFAULT_TIMEOUT,
            on_close: Box::new(on_close),
        }
    }

    #[must_use]
    pub fn timeout(self, seconds: u64) -> Self {
        Self {
            timeout_secs: seconds,
            ..self
        }
    }
}

impl<'a, Message, Theme> Widget<Message, Theme, Renderer> for Manager<'a, Message, Theme> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn tag(&self) -> widget::tree::Tag {
        struct Marker;
        widget::tree::Tag::of::<Marker>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(Vec::<Option<Instant>>::new())
    }

    fn children(&self) -> Vec<Tree> {
        std::iter::once(Tree::new(&self.content))
            .chain(self.toasts.iter().map(Tree::new))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let instants = tree.state.downcast_mut::<Vec<Option<Instant>>>();

        // Invalidating removed instants to None allows us to remove
        // them here so that diffing for removed / new toast instants
        // is accurate
        instants.retain(Option::is_some);

        match (instants.len(), self.toasts.len()) {
            (old, new) if old > new => {
                instants.truncate(new);
            }
            (old, new) if old < new => {
                instants.extend(std::iter::repeat(Some(Instant::now())).take(new - old));
            }
            _ => {}
        }

        tree.diff_children(
            &std::iter::once(&self.content)
                .chain(self.toasts.iter())
                .collect::<Vec<_>>(),
        );
    }

    fn operate(
        &self,
        state: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<Message>,
    ) {
        operation.container(None, layout.bounds(), &mut |operation| {
            self.content
                .as_widget()
                .operate(&mut state.children[0], layout, renderer, operation);
        });
    }

    fn on_event(
        &mut self,
        state: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        self.content.as_widget_mut().on_event(
            &mut state.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        state: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &state.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        state: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let instants = state.state.downcast_mut::<Vec<Option<Instant>>>();

        let (content_state, toasts_state) = state.children.split_at_mut(1);

        let content = self.content.as_widget_mut().overlay(
            &mut content_state[0],
            layout,
            renderer,
            translation,
        );

        let toasts = (!self.toasts.is_empty()).then(|| {
            overlay::Element::new(Box::new(Overlay {
                toasts: &mut self.toasts,
                state: toasts_state,
                instants,
                on_close: &self.on_close,
                timeout_secs: self.timeout_secs,
            }))
        });
        let overlays = content.into_iter().chain(toasts).collect::<Vec<_>>();

        (!overlays.is_empty()).then(|| overlay::Group::with_children(overlays).overlay())
    }
}

struct Overlay<'a, 'b, Message, Theme> {
    toasts: &'b mut [Element<'a, Message, Theme>],
    state: &'b mut [Tree],
    instants: &'b mut [Option<Instant>],
    on_close: &'b dyn Fn(usize) -> Message,
    timeout_secs: u64,
}

impl<'a, 'b, Message, Theme> overlay::Overlay<Message, Theme, Renderer>
    for Overlay<'a, 'b, Message, Theme>
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let limits = layout::Limits::new(Size::ZERO, bounds)
            .width(Length::Fill)
            .height(Length::Fill);

        layout::flex::resolve(
            layout::flex::Axis::Vertical,
            renderer,
            &limits,
            Length::Fill,
            Length::Fill,
            10.into(),
            10.0,
            Alignment::End,
            self.toasts,
            self.state,
        )
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let viewport = layout.bounds();

        for ((child, state), layout) in self
            .toasts
            .iter()
            .zip(self.state.iter())
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, &viewport);
        }
    }

    fn operate(
        &mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation<Message>,
    ) {
        operation.container(None, layout.bounds(), &mut |operation| {
            self.toasts
                .iter()
                .zip(self.state.iter_mut())
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget()
                        .operate(state, layout, renderer, operation);
                });
        });
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        if let Event::Window(_, window::Event::RedrawRequested(now)) = &event {
            let mut next_redraw: Option<window::RedrawRequest> = None;

            self.instants
                .iter_mut()
                .enumerate()
                .for_each(|(index, maybe_instant)| {
                    if index == 0 {
                        // The horizontal space at the start
                        return;
                    }

                    if let Some(instant) = maybe_instant.as_mut() {
                        let remaining = Duration::from_secs(self.timeout_secs)
                            .saturating_sub(instant.elapsed());

                        if remaining == Duration::ZERO {
                            maybe_instant.take();
                            shell.publish((self.on_close)(index - 1));
                            next_redraw = Some(window::RedrawRequest::NextFrame);
                        } else {
                            let redraw_at = window::RedrawRequest::At(*now + remaining);
                            next_redraw = next_redraw
                                .map(|redraw| redraw.min(redraw_at))
                                .or(Some(redraw_at));
                        }
                    }
                });

            if let Some(redraw) = next_redraw {
                shell.request_redraw(redraw);
            }
        }

        let viewport = layout.bounds();

        self.toasts
            .iter_mut()
            .zip(self.state.iter_mut())
            .zip(layout.children())
            .zip(self.instants.iter_mut())
            .map(|(((child, state), layout), instant)| {
                let mut local_messages = vec![];
                let mut local_shell = Shell::new(&mut local_messages);

                let status = child.as_widget_mut().on_event(
                    state,
                    event.clone(),
                    layout,
                    cursor,
                    renderer,
                    clipboard,
                    &mut local_shell,
                    &viewport,
                );

                if !local_shell.is_empty() {
                    instant.take();
                }

                shell.merge(local_shell, std::convert::identity);

                status
            })
            .fold(event::Status::Ignored, event::Status::merge)
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.toasts
            .iter()
            .zip(self.state.iter())
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, viewport, renderer)
            })
            .max()
            .unwrap_or_default()
    }

    fn is_over(&self, layout: Layout<'_>, _renderer: &Renderer, cursor_position: Point) -> bool {
        layout
            .children()
            .any(|layout| layout.bounds().contains(cursor_position))
    }
}

impl<'a, Message, Theme> From<Manager<'a, Message, Theme>> for Element<'a, Message, Theme>
where
    Message: 'a,
    Theme: 'a,
{
    fn from(manager: Manager<'a, Message, Theme>) -> Self {
        Element::new(manager)
    }
}
