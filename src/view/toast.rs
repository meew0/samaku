//! Code for showing toasts (dismissable notifications) over the UI.
//!
//! Mostly copied from the iced `toast` example: https://github.com/iced-rs/iced/blob/bc9bb28b1ccd1248d63ccdfef2f57d7aa837abbb/examples/toast/src/main.rs.

use std::time::{Duration, Instant};

use iced::advanced::layout::{self, Layout};
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{self, Operation, Tree};
use iced::advanced::{Clipboard, Shell, Widget};
use iced::event::Event;
use iced::mouse;
use iced::window;
use iced::{Alignment, Element, Length, Rectangle, Renderer, Size, Theme, Vector};

use crate::model::toast;

// Re-export model types so existing callers via `view::toast::Status` / `view::toast::Toast`
// continue to work without changes.
pub use toast::List;

pub const DEFAULT_TIMEOUT: u64 = 5;

fn make_style(status: toast::Status) -> impl Fn(&Theme) -> iced::widget::container::Style {
    move |theme| {
        let palette = theme.extended_palette();

        let pair = match status {
            toast::Status::Primary => palette.primary.weak,
            toast::Status::Secondary => palette.secondary.weak,
            toast::Status::Success => palette.success.weak,
            toast::Status::Danger => palette.danger.weak,
        };

        iced::widget::container::Style {
            background: Some(pair.color.into()),
            text_color: pair.text.into(),
            ..Default::default()
        }
    }
}

pub struct Manager<'a, Message> {
    content: Element<'a, Message>,
    /// Pre-built iced elements: one leading vertical spacer + one per toast.
    elements: Vec<Element<'a, Message>>,
    /// Per-toast effective timeout in seconds (index matches toast slice).
    timeouts: Vec<u64>,
    default_timeout_secs: u64,
    on_close: Box<dyn Fn(usize) -> Message + 'a>,
}

impl<'a, Message> Manager<'a, Message>
where
    Message: 'a + Clone,
{
    /// Create a new `Manager`.
    /// `on_close(index)` is published when a toast is dismissed (by timeout or the X button).
    pub fn new<E, F>(content: E, toasts: &'a [toast::Toast<Message>], on_close: F) -> Self
    where
        E: Into<Element<'a, Message, Theme>>,
        F: Fn(usize) -> Message + 'a,
    {
        let mut elements: Vec<Element<'a, Message, Theme>> = vec![];

        // In samaku, we want the toasts to appear at the bottom, so add a vertical space.
        elements.push(iced::widget::space::vertical().into());

        let mut timeouts: Vec<u64> = Vec::with_capacity(toasts.len());

        for (index, toast) in toasts.iter().enumerate() {
            timeouts.push(toast.timeout_secs.unwrap_or(DEFAULT_TIMEOUT));

            let title_text = if toast.count == 1 {
                iced::widget::text(toast.title.as_str())
            } else {
                iced::widget::text(format!("({}x) {}", toast.count, toast.title))
            };

            let body_area: Element<'a, Message> = match &toast.content {
                toast::Content::Message => {
                    iced::widget::container(iced::widget::text(toast.body.as_str()))
                        .width(Length::Fill)
                        .padding(5)
                        .style(iced::widget::container::rounded_box)
                        .into()
                }

                toast::Content::Progress { progress } => iced::widget::container(
                    iced::widget::column![
                        iced::widget::text(toast.body.as_str()),
                        iced::widget::progress_bar(0.0..=1.0, *progress),
                    ]
                    .spacing(4),
                )
                .width(Length::Fill)
                .padding(5)
                .style(iced::widget::container::rounded_box)
                .into(),

                toast::Content::Confirm {
                    confirm_label,
                    deny_label,
                    ..
                } => iced::widget::container(
                    iced::widget::column![
                        iced::widget::text(toast.body.as_str()),
                        iced::widget::row![
                            // TODO: these buttons don't do anything yet
                            iced::widget::button(confirm_label.as_str()).padding(3),
                            iced::widget::button(deny_label.as_str()).padding(3),
                        ]
                        .spacing(4),
                    ]
                    .spacing(4),
                )
                .width(Length::Fill)
                .padding(5)
                .style(iced::widget::container::rounded_box)
                .into(),
            };

            elements.push(
                iced::widget::container(iced::widget::column![
                    iced::widget::container(
                        iced::widget::row![
                            title_text,
                            iced::widget::space::horizontal(),
                            iced::widget::button("X")
                                .on_press(on_close(index))
                                .padding(3),
                        ]
                        .align_y(Alignment::Center)
                    )
                    .width(Length::Fill)
                    .padding(5)
                    .style(make_style(toast.status)),
                    iced::widget::rule::horizontal(1),
                    body_area,
                ])
                .max_width(200)
                .into(),
            );
        }

        Self {
            content: content.into(),
            elements,
            timeouts,
            default_timeout_secs: DEFAULT_TIMEOUT,
            on_close: Box::new(on_close),
        }
    }

    #[must_use]
    pub fn timeout(self, seconds: u64) -> Self {
        Self {
            default_timeout_secs: seconds,
            ..self
        }
    }
}

impl<Message> Widget<Message, Theme, Renderer> for Manager<'_, Message> {
    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &mut self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget_mut()
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
            .chain(self.elements.iter().map(Tree::new))
            .collect()
    }

    fn diff(&self, tree: &mut Tree) {
        let instants = tree.state.downcast_mut::<Vec<Option<Instant>>>();

        // Invalidating removed instants to None allows us to remove
        // them here so that diffing for removed / new toast instants
        // is accurate
        instants.retain(Option::is_some);

        match (instants.len(), self.elements.len()) {
            (old, new) if old > new => {
                instants.truncate(new);
            }
            (old, new) if old < new => {
                instants.extend(std::iter::repeat_n(Some(Instant::now()), new - old));
            }
            _ => {}
        }

        tree.diff_children(
            &std::iter::once(&self.content)
                .chain(self.elements.iter())
                .collect::<Vec<_>>(),
        );
    }

    fn operate(
        &mut self,
        state: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.content.as_widget_mut().operate(
                &mut state.children[0],
                layout,
                renderer,
                operation,
            );
        });
    }

    fn update(
        &mut self,
        state: &mut Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        self.content.as_widget_mut().update(
            &mut state.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        );
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
        layout: Layout<'b>,
        renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let instants = state.state.downcast_mut::<Vec<Option<Instant>>>();

        let (content_state, toasts_state) = state.children.split_at_mut(1);

        let content = self.content.as_widget_mut().overlay(
            &mut content_state[0],
            layout,
            renderer,
            viewport,
            translation,
        );

        let toasts = (!self.elements.is_empty()).then(|| {
            overlay::Element::new(Box::new(Overlay {
                viewport: *viewport,
                elements: &mut self.elements,
                state: toasts_state,
                instants,
                on_close: &self.on_close,
                timeouts: &self.timeouts,
                default_timeout_secs: self.default_timeout_secs,
            }))
        });
        let overlays = content.into_iter().chain(toasts).collect::<Vec<_>>();

        (!overlays.is_empty()).then(move || overlay::Group::with_children(overlays).overlay())
    }
}

struct Overlay<'a, 'b, Message> {
    viewport: Rectangle,
    elements: &'b mut [Element<'a, Message>],
    state: &'b mut [Tree],
    instants: &'b mut [Option<Instant>],
    on_close: &'b dyn Fn(usize) -> Message,
    /// Per-toast effective timeout in seconds (index matches toast slice, not elements).
    timeouts: &'b [u64],
    default_timeout_secs: u64,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer> for Overlay<'_, '_, Message> {
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
            self.elements,
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
            .elements
            .iter()
            .zip(self.state.iter())
            .zip(layout.children())
        {
            child
                .as_widget()
                .draw(state, renderer, theme, style, layout, cursor, &viewport);
        }
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        operation.container(None, layout.bounds());
        operation.traverse(&mut |operation| {
            self.elements
                .iter_mut()
                .zip(self.state.iter_mut())
                .zip(layout.children())
                .for_each(|((child, state), layout)| {
                    child
                        .as_widget_mut()
                        .operate(state, layout, renderer, operation);
                });
        });
    }

    fn update(
        &mut self,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) {
        if let Event::Window(window::Event::RedrawRequested(now)) = &event {
            let mut next_redraw: Option<window::RedrawRequest> = None;

            self.instants
                .iter_mut()
                .enumerate()
                .for_each(|(index, maybe_instant)| {
                    if index == 0 {
                        // The vertical space at the start
                        return;
                    }

                    if let Some(instant) = maybe_instant.as_mut() {
                        // `index - 1` maps from elements index (skip leading spacer) to toast
                        // data index. Fall back to the default if somehow out of range.
                        let toast_index = index - 1;
                        let timeout_secs = self
                            .timeouts
                            .get(toast_index)
                            .copied()
                            .unwrap_or(self.default_timeout_secs);

                        let remaining =
                            Duration::from_secs(timeout_secs).saturating_sub(instant.elapsed());

                        if remaining == Duration::ZERO {
                            maybe_instant.take();
                            shell.publish((self.on_close)(toast_index));
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
                shell.request_redraw_at(redraw);
            }
        }

        let viewport = layout.bounds();

        for (((child, state), layout), instant) in self
            .elements
            .iter_mut()
            .zip(self.state.iter_mut())
            .zip(layout.children())
            .zip(self.instants.iter_mut())
        {
            let mut local_messages = vec![];
            let mut local_shell = Shell::new(&mut local_messages);

            child.as_widget_mut().update(
                state,
                event,
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
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.elements
            .iter()
            .zip(self.state.iter())
            .zip(layout.children())
            .map(|((child, state), layout)| {
                child
                    .as_widget()
                    .mouse_interaction(state, layout, cursor, &self.viewport, renderer)
                    .max(if cursor.is_over(layout.bounds()) {
                        mouse::Interaction::Idle
                    } else {
                        mouse::Interaction::default()
                    })
            })
            .max()
            .unwrap_or_default()
    }
}

impl<'a, Message> From<Manager<'a, Message>> for Element<'a, Message>
where
    Message: 'a,
{
    fn from(manager: Manager<'a, Message>) -> Self {
        Element::new(manager)
    }
}
