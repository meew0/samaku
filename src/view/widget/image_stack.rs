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
pub struct ImageStack<H, M, P, R>
where
    P: canvas::Program<M, R>,
    R: image::Renderer<Handle = H> + canvas::Renderer,
{
    images: Vec<StackedImage<H>>,
    width: Length,
    height: Length,
    content_fit: ContentFit,
    program: P,
    _phantom_message: PhantomData<M>,
    _phantom_renderer: PhantomData<R>,
}

impl<H, M, P, R> ImageStack<H, M, P, R>
where
    P: canvas::Program<M, R>,
    R: image::Renderer<Handle = H> + canvas::Renderer,
{
    /// Creates a new [`ImageStack`] with the given path.
    pub fn new<T: Into<Vec<StackedImage<H>>>>(images: T, program: P) -> Self {
        ImageStack {
            images: images.into(),
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
            program,
            _phantom_message: PhantomData,
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
}

/// Computes the layout of an [`ImageStack`].
pub fn layout<R, H>(
    renderer: &R,
    limits: &layout::Limits,
    images: &[StackedImage<H>],
    width: Length,
    height: Length,
    content_fit: ContentFit,
) -> layout::Node
where
    R: image::Renderer<Handle = H>,
{
    // The raw w/h of the first image
    let image_size = {
        let Size { width, height } = renderer.dimensions(&images[0].handle);

        #[allow(clippy::cast_precision_loss)]
        Size::new(width as f32, height as f32)
    };

    // The size to be available to the widget prior to `Shrink`ing
    let raw_size = limits.width(width).height(height).resolve(image_size);

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
pub fn draw<R, H>(
    renderer: &mut R,
    layout: Layout<'_>,
    images: &[StackedImage<H>],
    content_fit: ContentFit,
) where
    R: image::Renderer<Handle = H>,
    H: Clone + Hash,
{
    // Find out maximum size (assuming the first image covers the entire area)
    let Size { width, height } = renderer.dimensions(&images[0].handle);
    #[allow(clippy::cast_precision_loss)]
    let overall_size = Size::new(width as f32, height as f32);

    let bounds = layout.bounds();
    let overall_adjusted_fit = content_fit.fit(overall_size, bounds.size());

    let x_scale = overall_adjusted_fit.width / overall_size.width;
    let y_scale = overall_adjusted_fit.height / overall_size.height;

    // Preprocess images
    let to_draw: Vec<(&StackedImage<H>, Rectangle)> = images
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
    let render = |renderer: &mut R| {
        for (image, bounds) in to_draw {
            renderer.draw(image.handle.clone(), bounds);
        }
    };

    if overall_adjusted_fit.width > bounds.width || overall_adjusted_fit.height > bounds.height {
        renderer.with_layer(bounds, render);
    } else {
        render(renderer);
    }
}

impl<M, R, H, P> Widget<M, R> for ImageStack<H, M, P, R>
where
    R: image::Renderer<Handle = H> + canvas::Renderer,
    H: Clone + Hash,
    P: canvas::Program<M, R>,
{
    fn width(&self) -> Length {
        self.width
    }

    fn height(&self) -> Length {
        self.height
    }

    fn layout(&self, renderer: &R, limits: &layout::Limits) -> layout::Node {
        layout(
            renderer,
            limits,
            &self.images,
            self.width,
            self.height,
            self.content_fit,
        )
    }

    fn draw(
        &self,
        state: &Tree,
        renderer: &mut R,
        theme: &R::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(renderer, layout, &self.images, self.content_fit);

        let bounds = layout.bounds();

        let state = state.state.downcast_ref::<P::State>();
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
        tree::Tag::of::<Tag<P::State>>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(P::State::default())
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &R,
        _clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, M>,
        _viewport: &Rectangle,
    ) -> iced::event::Status {
        let bounds = layout.bounds();

        let canvas_event = match event {
            Event::Mouse(mouse_event) => Some(canvas::Event::Mouse(mouse_event)),
            Event::Touch(touch_event) => Some(canvas::Event::Touch(touch_event)),
            Event::Keyboard(keyboard_event) => Some(canvas::Event::Keyboard(keyboard_event)),
            Event::Window(_) => None,
        };

        if let Some(canvas_event) = canvas_event {
            let state = tree.state.downcast_mut::<P::State>();

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
        _renderer: &R,
    ) -> mouse::Interaction {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<P::State>();

        self.program.mouse_interaction(state, bounds, cursor)
    }
}

impl<'a, M, R, H, P> From<ImageStack<H, M, P, R>> for Element<'a, M, R>
where
    R: image::Renderer<Handle = H> + geometry::Renderer + 'a,
    H: Clone + Hash + 'a,
    P: canvas::Program<M, R> + 'a,
    M: 'a,
{
    fn from(image: ImageStack<H, M, P, R>) -> Element<'a, M, R> {
        Element::new(image)
    }
}
