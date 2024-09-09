use std::hash::Hash;
use std::marker::PhantomData;

use iced::advanced::graphics::geometry;
use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::{tree, Tree};
use iced::advanced::{image, Clipboard, Shell};
use iced::advanced::{Layout, Widget};
use iced::widget::canvas;
use iced::{ContentFit, Element, Event, Length, Rectangle, Size, Vector};

#[derive(Debug)]
pub struct StackedImage<H> {
    pub handle: H,
    pub x: i32,
    pub y: i32,
}

/// Displays a stack of images overlaid on top of each other.
/// The size is defined by the first image in the stack
#[derive(Debug)]
pub struct ImageStack<Handle, Message, Program, Theme, Renderer>
where
    Program: canvas::Program<Message, Theme, Renderer>,
    Renderer: image::Renderer<Handle = Handle> + canvas::Renderer,
{
    images: Vec<StackedImage<Handle>>,
    image_size_override: Option<Size<u32>>,
    width: Length,
    height: Length,
    content_fit: ContentFit,
    program: Program,
    _phantom_message: PhantomData<Message>,
    _phantom_theme: PhantomData<Theme>,
    _phantom_renderer: PhantomData<Renderer>,
}

impl<Handle, Message, Program, Theme, Renderer>
    ImageStack<Handle, Message, Program, Theme, Renderer>
where
    Program: canvas::Program<Message, Theme, Renderer>,
    Renderer: image::Renderer<Handle = Handle> + canvas::Renderer,
{
    /// Creates a new [`ImageStack`] with the given path.
    pub fn new<T: Into<Vec<StackedImage<Handle>>>>(images: T, program: Program) -> Self {
        ImageStack {
            images: images.into(),
            image_size_override: None,
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
            program,
            _phantom_message: PhantomData,
            _phantom_theme: PhantomData,
            _phantom_renderer: PhantomData,
        }
    }

    /// Sets the width of the [`ImageStack`] boundaries.
    #[must_use]
    pub fn set_stack_width<L: Into<Length>>(mut self, width: L) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`ImageStack`] boundaries.
    #[must_use]
    pub fn set_stack_height<L: Into<Length>>(mut self, height: L) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`ContentFit`] of the [`ImageStack`].
    ///
    /// Defaults to [`ContentFit::Contain`]
    #[must_use]
    pub fn content_fit(self, content_fit: ContentFit) -> Self {
        Self {
            content_fit,
            ..self
        }
    }

    /// By default, the image size of the [`ImageStack`] is measured by the first image in the stack. If another size is desired, this
    /// method can be used to override that measurement.
    #[must_use]
    pub fn set_image_size_override(mut self, image_size_override: Size<u32>) -> Self {
        self.image_size_override = Some(image_size_override);
        self
    }
}

/// Computes the layout of an [`ImageStack`].
pub fn layout<Renderer, Handle>(
    renderer: &Renderer,
    limits: &layout::Limits,
    images: &[StackedImage<Handle>],
    width: Length,
    height: Length,
    content_fit: ContentFit,
    image_size_override: Option<Size<u32>>,
) -> layout::Node
where
    Renderer: image::Renderer<Handle = Handle>,
{
    // The raw w/h of the first image, or the override, if specified
    let image_size = {
        let Size { width, height } =
            image_size_override.unwrap_or_else(|| renderer.dimensions(&images[0].handle));

        #[allow(clippy::cast_precision_loss)]
        Size::new(width as f32, height as f32)
    };

    // The size to be available to the widget prior to `Shrink`ing
    let raw_size = limits
        .width(width)
        .height(height)
        .resolve(width, height, image_size);

    // The uncropped size of the image when fit to the bounds above
    let full_size = content_fit.fit(image_size, raw_size);

    // Shrink the widget to fit the resized image, if requested
    let final_size = Size {
        width: match width {
            Length::Shrink => f32::min(raw_size.width, full_size.width),
            _ => raw_size.width,
        },
        height: match height {
            Length::Shrink => f32::min(raw_size.height, full_size.height),
            _ => raw_size.height,
        },
    };

    layout::Node::new(final_size)
}

/// Draws an [`ImageStack`]
pub fn draw<Renderer, Handle>(
    renderer: &mut Renderer,
    layout: Layout<'_>,
    images: &[StackedImage<Handle>],
    content_fit: ContentFit,
    image_size_override: Option<Size<u32>>,
) where
    Renderer: image::Renderer<Handle = Handle>,
    Handle: Clone + Hash,
{
    // Find out maximum size (assuming the first image covers the entire area)
    let Size { width, height } =
        image_size_override.unwrap_or_else(|| renderer.dimensions(&images[0].handle));
    #[allow(clippy::cast_precision_loss)]
    let overall_size = Size::new(width as f32, height as f32);

    let bounds = layout.bounds();
    let overall_adjusted_fit = content_fit.fit(overall_size, bounds.size());

    let x_scale = overall_adjusted_fit.width / overall_size.width;
    let y_scale = overall_adjusted_fit.height / overall_size.height;

    // Preprocess images
    let to_draw: Vec<(&StackedImage<Handle>, Rectangle)> = images
        .iter()
        .map(|image| {
            let Size { width, height } = renderer.dimensions(&image.handle);

            let center_offset = Vector::new(
                (bounds.width - overall_adjusted_fit.width).max(0.0) / 2.0,
                (bounds.height - overall_adjusted_fit.height).max(0.0) / 2.0,
            );

            #[allow(clippy::cast_precision_loss)]
            let pos_offset = Vector::new((image.x as f32) * x_scale, (image.y as f32) * y_scale);

            #[allow(clippy::cast_precision_loss)]
            let drawing_bounds = Rectangle {
                width: width as f32 * x_scale,
                height: height as f32 * y_scale,
                ..bounds
            };

            let sum_bounds = drawing_bounds + center_offset + pos_offset;

            (image, sum_bounds)
        })
        .collect();
    let render = |renderer: &mut Renderer| {
        for (image, bounds) in to_draw {
            renderer.draw(image.handle.clone(), image::FilterMethod::Linear, bounds);
        }
    };

    if overall_adjusted_fit.width > bounds.width || overall_adjusted_fit.height > bounds.height {
        renderer.with_layer(bounds, render);
    } else {
        render(renderer);
    }
}

impl<Message, Renderer, Handle, Program, Theme> Widget<Message, Theme, Renderer>
    for ImageStack<Handle, Message, Program, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle> + canvas::Renderer,
    Handle: Clone + Hash,
    Program: canvas::Program<Message, Theme, Renderer>,
{
    fn size(&self) -> Size<Length> {
        Size {
            width: self.width,
            height: self.height,
        }
    }

    fn layout(
        &self,
        _state: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout(
            renderer,
            limits,
            &self.images,
            self.width,
            self.height,
            self.content_fit,
            self.image_size_override,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(
            renderer,
            layout,
            &self.images,
            self.content_fit,
            self.image_size_override,
        );

        let bounds = layout.bounds();

        let state = tree.state.downcast_ref::<Program::State>();
        renderer.with_layer(bounds, |renderer| {
            renderer.with_translation(Vector::new(bounds.x, bounds.y), |renderer| {
                canvas::Renderer::draw(
                    renderer,
                    self.program
                        .draw(state, renderer, theme, layout.bounds(), cursor),
                );
            });
        });
    }

    fn tag(&self) -> tree::Tag {
        struct Tag<T>(T);
        tree::Tag::of::<Tag<Program::State>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(Program::State::default())
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> iced::event::Status {
        let bounds = layout.bounds();

        let canvas_event = match event {
            Event::Mouse(mouse_event) => Some(canvas::Event::Mouse(mouse_event)),
            Event::Touch(touch_event) => Some(canvas::Event::Touch(touch_event)),
            Event::Keyboard(keyboard_event) => Some(canvas::Event::Keyboard(keyboard_event)),
            Event::Window(..) => None,
        };

        if let Some(canvas_event) = canvas_event {
            let state = tree.state.downcast_mut::<Program::State>();

            let (event_status, message) = self.program.update(state, canvas_event, bounds, cursor);

            if let Some(message) = message {
                shell.publish(message);
            }

            return event_status;
        }

        iced::event::Status::Ignored
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<Program::State>();

        self.program.mouse_interaction(state, bounds, cursor)
    }
}

impl<'a, Message, Renderer, Handle, Program, Theme>
    From<ImageStack<Handle, Message, Program, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Renderer: image::Renderer<Handle = Handle> + geometry::Renderer + 'a,
    Handle: Clone + Hash + 'a,
    Program: canvas::Program<Message, Theme, Renderer> + 'a,
    Message: 'a,
    Theme: 'a,
{
    fn from(
        image: ImageStack<Handle, Message, Program, Theme, Renderer>,
    ) -> Element<'a, Message, Theme, Renderer> {
        Element::new(image)
    }
}

pub struct EmptyProgram;

impl<Message, Theme, Renderer> canvas::Program<Message, Theme, Renderer> for EmptyProgram
where
    Renderer: canvas::Renderer,
{
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        _renderer: &Renderer,
        _theme: &Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<<Renderer as canvas::Renderer>::Geometry> {
        vec![]
    }

    fn update(
        &self,
        _state: &mut Self::State,
        _event: canvas::Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> (canvas::event::Status, Option<Message>) {
        (canvas::event::Status::Ignored, None)
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        mouse::Interaction::default()
    }
}
