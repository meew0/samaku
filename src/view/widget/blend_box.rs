use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::keyboard;
use iced::keyboard::key;
use iced::widget::overlay::menu;
use iced::widget::text::LineHeight;
use iced::widget::text_input::{self, TextInput};
use iced::{Element, Event, Length, Padding, Pixels, Rectangle, Size, Vector};

use crate::model;
use std::cell::RefCell;

/// A searchable dropdown widget that takes an external `&[T]` slice, keeping
/// only the text-input search state in a separate, non-generic [`State`].
pub struct BlendBox<'a, L, Message, Theme = iced::Theme, Renderer = iced::Renderer>
where
    L: model::NamedListIterable + ?Sized,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    state: &'a State,
    options: &'a L,
    text_input: TextInput<'a, TextInputEvent, Theme, Renderer>,
    font: Option<Renderer::Font>,
    selection: text_input::Value,
    on_selected: Box<dyn Fn(L::Key) -> Message>,
    on_option_hovered: Option<Box<HoverFn<L::Key, Message>>>,
    on_open: Option<Message>,
    on_close: Option<Message>,
    on_input: Option<Box<dyn Fn(String) -> Message>>,
    padding: Padding,
    size: Option<f32>,
    text_shaping: text::Shaping,
    menu_class: <Theme as menu::Catalog>::Class<'a>,
    menu_height: Length,
}

type HoverFn<K, Message> = dyn Fn(Reference<K>) -> Message;

impl<'a, L, Message, Theme, Renderer> BlendBox<'a, L, Message, Theme, Renderer>
where
    L: model::NamedListIterable + ?Sized,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    pub fn new<F: Fn(L::Key) -> Message + 'static>(
        state: &'a State,
        options: &'a L,
        placeholder: &str,
        selection_opt: Option<model::NamedEntry<L::Key>>,
        on_selected: F,
    ) -> Self {
        let text_input = TextInput::new(placeholder, &state.value())
            .on_input(TextInputEvent::TextChanged)
            .class(Theme::default_input());

        let selection = selection_opt
            .map(|entry| entry.name)
            .unwrap_or_default()
            .to_owned();

        Self {
            state,
            options,
            text_input,
            font: None,
            selection: text_input::Value::new(&selection),
            on_selected: Box::new(on_selected),
            on_option_hovered: None,
            on_input: None,
            on_open: None,
            on_close: None,
            padding: text_input::DEFAULT_PADDING,
            size: None,
            text_shaping: text::Shaping::default(),
            menu_class: <Theme as Catalog>::default_menu(),
            menu_height: Length::Shrink,
        }
    }

    #[must_use]
    pub fn on_input<F: Fn(String) -> Message + 'static>(mut self, on_input: F) -> Self {
        self.on_input = Some(Box::new(on_input));
        self
    }

    #[must_use]
    pub fn on_option_hovered<F: Fn(L::Key) -> Message + 'static>(
        mut self,
        on_option_hovered: F,
    ) -> Self {
        self.on_option_hovered = Some(Box::new(move |reference| {
            on_option_hovered(reference.index)
        }));
        self
    }

    #[must_use]
    pub fn on_open(mut self, message: Message) -> Self {
        self.on_open = Some(message);
        self
    }

    #[must_use]
    pub fn on_close(mut self, message: Message) -> Self {
        self.on_close = Some(message);
        self
    }

    #[must_use]
    pub fn padding<P: Into<Padding>>(mut self, padding: P) -> Self {
        self.padding = padding.into();
        self.text_input = self.text_input.padding(self.padding);
        self
    }

    #[must_use]
    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.text_input = self.text_input.font(font);
        self.font = Some(font);
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: text_input::Icon<Renderer::Font>) -> Self {
        self.text_input = self.text_input.icon(icon);
        self
    }

    #[must_use]
    pub fn size<P: Into<Pixels>>(mut self, into_size: P) -> Self {
        let size = into_size.into();
        self.text_input = self.text_input.size(size);
        self.size = Some(size.0);
        self
    }

    #[must_use]
    pub fn line_height<H: Into<LineHeight>>(self, line_height: H) -> Self {
        Self {
            text_input: self.text_input.line_height(line_height),
            ..self
        }
    }

    #[must_use]
    pub fn width<W: Into<Length>>(self, width: W) -> Self {
        Self {
            text_input: self.text_input.width(width),
            ..self
        }
    }

    #[must_use]
    pub fn menu_height<H: Into<Length>>(mut self, menu_height: H) -> Self {
        self.menu_height = menu_height.into();
        self
    }

    #[must_use]
    pub fn text_shaping(mut self, shaping: text::Shaping) -> Self {
        self.text_shaping = shaping;
        self
    }

    #[must_use]
    pub fn input_style<F: Fn(&Theme, text_input::Status) -> text_input::Style + 'a>(
        mut self,
        style: F,
    ) -> Self
    where
        <Theme as text_input::Catalog>::Class<'a>: From<text_input::StyleFn<'a, Theme>>,
    {
        self.text_input = self.text_input.style(style);
        self
    }

    #[must_use]
    pub fn menu_style<F: Fn(&Theme) -> menu::Style + 'a>(mut self, style: F) -> Self
    where
        <Theme as menu::Catalog>::Class<'a>: From<menu::StyleFn<'a, Theme>>,
    {
        let style_fn: menu::StyleFn<'a, Theme> = Box::new(style);
        self.menu_class = style_fn.into();
        self
    }

    // Utility methods for `update`
    fn try_publish_on_hovered(
        &mut self,
        menu: &Menu<L::Key>,
        shell: &mut Shell<'_, Message>,
    ) -> bool {
        if let &mut Some(ref mut on_option_hovered) = &mut self.on_option_hovered
            && let Some(option) = menu
                .hovered_option
                .and_then(|i| menu.filtered_options.get(i))
        {
            // Since we don't actually need the name in the `on_option_hovered` handler,
            // we can avoid unnecessarily cloning it.
            shell.publish(on_option_hovered(Reference {
                index: option.index,
                name: String::new(),
            }));
            return true;
        }

        false
    }
}

/// The state of a [`BlendBox`]. Holds only the current search-text value;
/// the option list lives outside in the caller's data.
#[derive(Debug, Clone, Default)]
pub struct State {
    inner: RefCell<Inner>,
}

#[derive(Debug, Clone, Default)]
struct Inner {
    value: String,
}

impl State {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn value(&self) -> String {
        self.inner.borrow().value.clone()
    }

    fn set_value(&self, value: String) {
        self.inner.borrow_mut().value = value;
    }

    fn clear(&self) {
        self.inner.borrow_mut().value = String::new();
    }
}

// Internal widget-tree state — one per BlendBox instance in the tree.
struct Menu<K: Copy> {
    menu: menu::State,
    hovered_option: Option<usize>,
    new_selection: Option<K>,
    filtered_options: Vec<Reference<K>>,
}

#[derive(Debug, Clone)]
enum TextInputEvent {
    TextChanged(String),
}

impl<L, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for BlendBox<'_, L, Message, Theme, Renderer>
where
    L: model::NamedListIterable + ?Sized + 'static,
    Message: Clone,
    Theme: Catalog,
    Renderer: text::Renderer,
{
    fn size(&self) -> Size<Length> {
        Widget::<TextInputEvent, Theme, Renderer>::size(&self.text_input)
    }

    fn layout(
        &mut self,
        tree: &mut widget::Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        self.text_input.layout(
            &mut tree.children[0],
            renderer,
            limits,
            (!is_focused).then_some(&self.selection),
        )
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        let selection = if is_focused || self.selection.is_empty() {
            None
        } else {
            Some(&self.selection)
        };

        self.text_input.draw(
            &tree.children[0],
            renderer,
            theme,
            layout,
            cursor,
            selection,
            viewport,
        );
    }

    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<Menu<L::Key>>()
    }

    fn state(&self) -> widget::tree::State {
        let value = self.state.value();
        let filtered_options = search(self.options, &value).collect();

        widget::tree::State::new(Menu {
            menu: menu::State::new(),
            filtered_options,
            hovered_option: Some(0),
            new_selection: None,
        })
    }

    fn children(&self) -> Vec<widget::Tree> {
        let text_input_ref: &dyn Widget<TextInputEvent, Theme, Renderer> = &self.text_input;
        vec![widget::Tree::new(text_input_ref)]
    }

    fn diff(&self, _tree: &mut widget::Tree) {
        // Intentionally empty — do not clear children.
    }

    #[expect(
        clippy::too_many_lines,
        reason = "annoying and low priority to decouple" // TODO
    )]
    fn update(
        &mut self,
        tree: &mut widget::Tree,
        event: &Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) {
        let menu = tree.state.downcast_mut::<Menu<L::Key>>();

        // TODO maybe optimize this with some caching or whatever, or by only doing the search when necessary?
        let value = self.state.value();
        menu.filtered_options = search(self.options, &value).collect();

        let started_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        let mut published_message_to_shell = false;

        let mut local_messages = Vec::new();
        let mut local_shell = Shell::new(&mut local_messages);

        self.text_input.update(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            &mut local_shell,
            viewport,
        );

        if local_shell.is_event_captured() {
            shell.capture_event();
        }
        shell.request_redraw_at(local_shell.redraw_request());
        shell.request_input_method(local_shell.input_method());

        for message in local_messages {
            let TextInputEvent::TextChanged(new_value) = message;

            if let &Some(ref on_input) = &self.on_input {
                shell.publish(on_input(new_value.clone()));
            }

            menu.hovered_option = Some(0);
            menu.filtered_options = search(self.options, &new_value).collect();
            self.state.set_value(new_value);

            shell.invalidate_layout();
            shell.request_redraw();
        }

        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        if is_focused {
            if !started_focused
                && let &mut Some(ref mut on_option_hovered) = &mut self.on_option_hovered
            {
                let hovered_option = menu.hovered_option.unwrap_or(0);
                if let Some(option) = menu.filtered_options.get(hovered_option) {
                    shell.publish(on_option_hovered(Reference {
                        index: option.index,
                        name: String::new(),
                    }));
                    published_message_to_shell = true;
                }
            }

            if let &Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named_key),
                ref modifiers,
                ..
            }) = event
            {
                let shift_modifier = modifiers.shift();
                match (named_key, shift_modifier) {
                    (key::Named::Enter, _) => {
                        if let &Some(hovered_index) = &menu.hovered_option
                            && let Some(reference) = menu.filtered_options.get(hovered_index)
                        {
                            menu.new_selection = Some(reference.index);
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    (key::Named::ArrowUp, _) | (key::Named::Tab, true) => {
                        if let &mut Some(ref mut index) = &mut menu.hovered_option {
                            if *index == 0 {
                                *index = menu.filtered_options.len().saturating_sub(1);
                            } else {
                                *index = index.saturating_sub(1);
                            }
                        } else {
                            menu.hovered_option = Some(0);
                        }

                        published_message_to_shell |= self.try_publish_on_hovered(menu, shell);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    (key::Named::ArrowDown, _) | (key::Named::Tab, false) if !modifiers.shift() => {
                        if let &mut Some(ref mut index) = &mut menu.hovered_option {
                            if *index >= menu.filtered_options.len().saturating_sub(1) {
                                *index = 0;
                            } else {
                                *index = index
                                    .saturating_add(1)
                                    .min(menu.filtered_options.len().saturating_sub(1));
                            }
                        } else {
                            menu.hovered_option = Some(0);
                        }

                        published_message_to_shell |= self.try_publish_on_hovered(menu, shell);
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    _ => {}
                }
            }
        }

        if let Some(selection) = menu.new_selection.take() {
            self.state.clear();
            menu.filtered_options = search(self.options, "").collect();
            menu.menu = menu::State::default();

            shell.publish((self.on_selected)(selection));
            published_message_to_shell = true;

            let mut inner_local_messages = Vec::new();
            let mut inner_local_shell = Shell::new(&mut inner_local_messages);
            self.text_input.update(
                &mut tree.children[0],
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                layout,
                mouse::Cursor::Unavailable,
                renderer,
                clipboard,
                &mut inner_local_shell,
                viewport,
            );
            shell.request_input_method(inner_local_shell.input_method());
        }

        let is_focused_after = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        if started_focused != is_focused_after {
            shell.invalidate_widgets();

            if !published_message_to_shell {
                if is_focused_after {
                    if let Some(on_open) = self.on_open.take() {
                        shell.publish(on_open);
                    }
                } else if let Some(on_close) = self.on_close.take() {
                    shell.publish(on_close);
                } else {
                    // menu closed, but no close handler defined
                }
            }
        }
    }

    fn mouse_interaction(
        &self,
        tree: &widget::Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.text_input
            .mouse_interaction(&tree.children[0], layout, cursor, viewport, renderer)
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut widget::Tree,
        layout: Layout<'_>,
        _renderer: &Renderer,
        viewport: &Rectangle,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        if !is_focused {
            return None;
        }

        let &mut Menu {
            ref mut menu,
            ref mut filtered_options,
            ref mut hovered_option,
            ..
        } = tree.state.downcast_mut::<Menu<L::Key>>();

        if filtered_options.is_empty() {
            return None;
        }

        let bounds = layout.bounds();

        let mut menu_widget = menu::Menu::new(
            menu,
            filtered_options,
            hovered_option,
            |selection| {
                self.state.clear();

                tree.children[0]
                    .state
                    .downcast_mut::<text_input::State<Renderer::Paragraph>>()
                    .unfocus();

                (self.on_selected)(selection.index)
            },
            self.on_option_hovered.as_deref(),
            &self.menu_class,
        )
        .width(bounds.width)
        .padding(self.padding)
        .text_shaping(self.text_shaping);

        if let Some(font) = self.font {
            menu_widget = menu_widget.font(font);
        }
        if let Some(size) = self.size {
            menu_widget = menu_widget.text_size(size);
        }

        Some(menu_widget.overlay(
            layout.position() + translation,
            *viewport,
            bounds.height,
            self.menu_height,
        ))
    }
}

impl<'a, L, Message, Theme, Renderer> From<BlendBox<'a, L, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    L: model::NamedListIterable + ?Sized + 'static,
    Message: Clone + 'a,
    Theme: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(blend_box: BlendBox<'a, L, Message, Theme, Renderer>) -> Self {
        Self::new(blend_box)
    }
}

/// The theme catalog for a [`BlendBox`].
pub trait Catalog: text_input::Catalog + menu::Catalog {
    #[must_use]
    fn default_input<'a>() -> <Self as text_input::Catalog>::Class<'a> {
        <Self as text_input::Catalog>::default()
    }

    #[must_use]
    fn default_menu<'a>() -> <Self as menu::Catalog>::Class<'a> {
        <Self as menu::Catalog>::default()
    }
}

impl Catalog for iced::Theme {}

fn search<'a, L>(options: &'a L, query: &'a str) -> impl Iterator<Item = Reference<L::Key>> + 'a
where
    L: model::NamedListIterable + ?Sized + 'a,
{
    // TODO: compare ignoring case
    options
        .iter_named()
        .filter(move |entry| entry.name.contains(query))
        .map(|entry| Reference {
            index: entry.id,
            name: entry.name.to_owned(),
        })
}

#[derive(Debug, Clone)]
struct Reference<K: Copy> {
    index: K,
    name: String,
}

impl<K> std::fmt::Display for Reference<K>
where
    K: Copy,
{
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
