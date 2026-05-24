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
use iced::{Element, Event, Length, Padding, Pixels, Rectangle, Size, Theme, Vector};

use crate::model;
use std::cell::RefCell;

/// A searchable dropdown widget that takes an external `&[T]` slice, keeping
/// only the text-input search state in a separate, non-generic [`State`].
pub struct BlendBox<'a, T, Message, Th = Theme, Renderer = iced::Renderer>
where
    Th: Catalog,
    Renderer: text::Renderer,
{
    state: &'a State,
    options: &'a [T],
    text_input: TextInput<'a, TextInputEvent, Th, Renderer>,
    font: Option<Renderer::Font>,
    selection: text_input::Value,
    on_selected: Box<dyn Fn(usize) -> Message>,
    on_option_hovered: Option<Box<dyn Fn(Reference) -> Message>>,
    on_open: Option<Message>,
    on_close: Option<Message>,
    on_input: Option<Box<dyn Fn(String) -> Message>>,
    padding: Padding,
    size: Option<f32>,
    text_shaping: text::Shaping,
    menu_class: <Th as menu::Catalog>::Class<'a>,
    menu_height: Length,
}

impl<'a, T, Message, Th, Renderer> BlendBox<'a, T, Message, Th, Renderer>
where
    T: model::Named + Clone,
    Th: Catalog,
    Renderer: text::Renderer,
{
    pub fn new(
        state: &'a State,
        options: &'a [T],
        placeholder: &str,
        selection: Option<&T>,
        on_selected: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        let text_input = TextInput::new(placeholder, &state.value())
            .on_input(TextInputEvent::TextChanged)
            .class(Th::default_input());

        let selection = selection.map(T::name).unwrap_or_default().to_owned();

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
            menu_class: <Th as Catalog>::default_menu(),
            menu_height: Length::Shrink,
        }
    }

    pub fn on_input(mut self, on_input: impl Fn(String) -> Message + 'static) -> Self {
        self.on_input = Some(Box::new(on_input));
        self
    }

    pub fn on_option_hovered(
        mut self,
        on_option_hovered: impl Fn(usize) -> Message + 'static,
    ) -> Self {
        self.on_option_hovered = Some(Box::new(move |reference| {
            on_option_hovered(reference.index)
        }));
        self
    }

    pub fn on_open(mut self, message: Message) -> Self {
        self.on_open = Some(message);
        self
    }

    pub fn on_close(mut self, message: Message) -> Self {
        self.on_close = Some(message);
        self
    }

    pub fn padding(mut self, padding: impl Into<Padding>) -> Self {
        self.padding = padding.into();
        self.text_input = self.text_input.padding(self.padding);
        self
    }

    pub fn font(mut self, font: Renderer::Font) -> Self {
        self.text_input = self.text_input.font(font);
        self.font = Some(font);
        self
    }

    pub fn icon(mut self, icon: text_input::Icon<Renderer::Font>) -> Self {
        self.text_input = self.text_input.icon(icon);
        self
    }

    pub fn size(mut self, size: impl Into<Pixels>) -> Self {
        let size = size.into();
        self.text_input = self.text_input.size(size);
        self.size = Some(size.0);
        self
    }

    pub fn line_height(self, line_height: impl Into<LineHeight>) -> Self {
        Self {
            text_input: self.text_input.line_height(line_height),
            ..self
        }
    }

    pub fn width(self, width: impl Into<Length>) -> Self {
        Self {
            text_input: self.text_input.width(width),
            ..self
        }
    }

    pub fn menu_height(mut self, menu_height: impl Into<Length>) -> Self {
        self.menu_height = menu_height.into();
        self
    }

    pub fn text_shaping(mut self, shaping: text::Shaping) -> Self {
        self.text_shaping = shaping;
        self
    }

    pub fn input_style(
        mut self,
        style: impl Fn(&Th, text_input::Status) -> text_input::Style + 'a,
    ) -> Self
    where
        <Th as text_input::Catalog>::Class<'a>: From<text_input::StyleFn<'a, Th>>,
    {
        self.text_input = self.text_input.style(style);
        self
    }

    pub fn menu_style(mut self, style: impl Fn(&Th) -> menu::Style + 'a) -> Self
    where
        <Th as menu::Catalog>::Class<'a>: From<menu::StyleFn<'a, Th>>,
    {
        let style_fn: menu::StyleFn<'a, Th> = Box::new(style);
        self.menu_class = style_fn.into();
        self
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
struct Menu {
    menu: menu::State,
    hovered_option: Option<usize>,
    new_selection: Option<usize>,
    filtered_options: Vec<Reference>,
}

#[derive(Debug, Clone)]
enum TextInputEvent {
    TextChanged(String),
}

impl<T, Message, Th, Renderer> Widget<Message, Th, Renderer>
    for BlendBox<'_, T, Message, Th, Renderer>
where
    T: model::Named + Clone + 'static,
    Message: Clone,
    Th: Catalog,
    Renderer: text::Renderer,
{
    fn size(&self) -> Size<Length> {
        Widget::<TextInputEvent, Th, Renderer>::size(&self.text_input)
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
        theme: &Th,
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
        widget::tree::Tag::of::<Menu>()
    }

    fn state(&self) -> widget::tree::State {
        let value = self.state.value();
        let filtered_options = search(self.options.iter(), &value).collect();

        widget::tree::State::new(Menu {
            menu: menu::State::new(),
            filtered_options,
            hovered_option: Some(0),
            new_selection: None,
        })
    }

    fn children(&self) -> Vec<widget::Tree> {
        let text_input_ref: &dyn Widget<TextInputEvent, Th, Renderer> = &self.text_input;
        vec![widget::Tree::new(text_input_ref)]
    }

    fn diff(&self, _tree: &mut widget::Tree) {
        // Intentionally empty — do not clear children.
    }

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
        let menu = tree.state.downcast_mut::<Menu>();

        let value = self.state.value();
        menu.filtered_options = search(self.options.iter(), &value).collect();

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

            if let Some(on_input) = &self.on_input {
                shell.publish((on_input)(new_value.clone()));
            }

            menu.hovered_option = Some(0);
            menu.filtered_options = search(self.options.iter(), &new_value).collect();
            self.state.set_value(new_value);

            shell.invalidate_layout();
            shell.request_redraw();
        }

        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        if is_focused {
            if !started_focused && let Some(on_option_hovered) = &mut self.on_option_hovered {
                let hovered_option = menu.hovered_option.unwrap_or(0);
                if let Some(option) = menu.filtered_options.get(hovered_option) {
                    shell.publish(on_option_hovered(Reference {
                        index: option.index,
                        name: String::new(),
                    }));
                    published_message_to_shell = true;
                }
            }

            if let Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(named_key),
                modifiers,
                ..
            }) = event
            {
                let shift_modifier = modifiers.shift();
                match (named_key, shift_modifier) {
                    (key::Named::Enter, _) => {
                        if let Some(hovered_index) = &menu.hovered_option
                            && let Some(reference) = menu.filtered_options.get(*hovered_index)
                        {
                            menu.new_selection = Some(reference.index);
                        }
                        shell.capture_event();
                        shell.request_redraw();
                    }
                    (key::Named::ArrowUp, _) | (key::Named::Tab, true) => {
                        if let Some(index) = &mut menu.hovered_option {
                            if *index == 0 {
                                *index = menu.filtered_options.len().saturating_sub(1);
                            } else {
                                *index = index.saturating_sub(1);
                            }
                        } else {
                            menu.hovered_option = Some(0);
                        }

                        if let Some(on_option_hovered) = &mut self.on_option_hovered
                            && let Some(option) = menu
                                .hovered_option
                                .and_then(|i| menu.filtered_options.get(i))
                        {
                            shell.publish((on_option_hovered)(option.clone()));
                            published_message_to_shell = true;
                        }

                        shell.capture_event();
                        shell.request_redraw();
                    }
                    (key::Named::ArrowDown, _) | (key::Named::Tab, false) if !modifiers.shift() => {
                        if let Some(index) = &mut menu.hovered_option {
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

                        if let Some(on_option_hovered) = &mut self.on_option_hovered
                            && let Some(option) = menu
                                .hovered_option
                                .and_then(|i| menu.filtered_options.get(i))
                        {
                            shell.publish((on_option_hovered)(option.clone()));
                            published_message_to_shell = true;
                        }

                        shell.capture_event();
                        shell.request_redraw();
                    }
                    _ => {}
                }
            }
        }

        if let Some(selection) = menu.new_selection.take() {
            self.state.clear();
            menu.filtered_options = self
                .options
                .iter()
                .enumerate()
                .map(|(i, option)| Reference {
                    index: i,
                    name: option.name().to_owned(),
                })
                .collect();
            menu.menu = menu::State::default();

            shell.publish((self.on_selected)(selection));
            published_message_to_shell = true;

            let mut local_messages = Vec::new();
            let mut local_shell = Shell::new(&mut local_messages);
            self.text_input.update(
                &mut tree.children[0],
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                layout,
                mouse::Cursor::Unavailable,
                renderer,
                clipboard,
                &mut local_shell,
                viewport,
            );
            shell.request_input_method(local_shell.input_method());
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
    ) -> Option<overlay::Element<'b, Message, Th, Renderer>> {
        let is_focused = tree.children[0]
            .state
            .downcast_ref::<text_input::State<Renderer::Paragraph>>()
            .is_focused();

        if !is_focused {
            return None;
        }

        let Menu {
            menu,
            filtered_options,
            hovered_option,
            ..
        } = tree.state.downcast_mut::<Menu>();

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

impl<'a, T, Message, Th, Renderer> From<BlendBox<'a, T, Message, Th, Renderer>>
    for Element<'a, Message, Th, Renderer>
where
    T: model::Named + Clone + 'static,
    Message: Clone + 'a,
    Th: Catalog + 'a,
    Renderer: text::Renderer + 'a,
{
    fn from(blend_box: BlendBox<'a, T, Message, Th, Renderer>) -> Self {
        Self::new(blend_box)
    }
}

/// The theme catalog for a [`BlendBox`].
pub trait Catalog: text_input::Catalog + menu::Catalog {
    fn default_input<'a>() -> <Self as text_input::Catalog>::Class<'a> {
        <Self as text_input::Catalog>::default()
    }

    fn default_menu<'a>() -> <Self as menu::Catalog>::Class<'a> {
        <Self as menu::Catalog>::default()
    }
}

impl Catalog for Theme {}

fn search<'a, T>(
    options: impl IntoIterator<Item = &'a T> + 'a,
    query: &'a str,
) -> impl Iterator<Item = Reference> + 'a
where
    T: model::Named + 'a,
{
    // TODO: compare ignoring case
    options
        .into_iter()
        .enumerate()
        .filter_map(move |(i, option)| {
            option.name().contains(query).then(|| Reference {
                index: i,
                name: option.name().to_owned(),
            })
        })
}

#[derive(Debug, Clone)]
struct Reference {
    index: usize,
    name: String,
}

impl std::fmt::Display for Reference {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}
