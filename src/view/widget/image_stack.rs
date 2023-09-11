use iced::advanced::image;
use iced::advanced::layout;
use iced::advanced::mouse;
use iced::advanced::renderer;
use iced::advanced::widget::Tree;
use iced::advanced::{Layout, Widget};
use iced::{ContentFit, Element, Length, Rectangle, Size, Vector};

use std::hash::Hash;

pub use image::Handle;

#[derive(Debug)]
pub struct StackedImage<H> {
    pub handle: H,
    pub x: i32,
    pub y: i32,
}

/// Displays a stack of images overlaid on top of each other.
/// The size is defined by the first image in the stack
#[derive(Debug)]
pub struct ImageStack<H> {
    images: Vec<StackedImage<H>>,
    width: Length,
    height: Length,
    content_fit: ContentFit,
}

impl<H> ImageStack<H> {
    /// Creates a new [`ImageStack`] with the given path.
    pub fn new<T: Into<Vec<StackedImage<H>>>>(images: T) -> Self {
        ImageStack {
            images: images.into(),
            width: Length::Shrink,
            height: Length::Shrink,
            content_fit: ContentFit::Contain,
        }
    }

    /// Sets the width of the [`ImageStack`] boundaries.
    pub fn width(mut self, width: impl Into<Length>) -> Self {
        self.width = width.into();
        self
    }

    /// Sets the height of the [`ImageStack`] boundaries.
    pub fn height(mut self, height: impl Into<Length>) -> Self {
        self.height = height.into();
        self
    }

    /// Sets the [`ContentFit`] of the [`ImageStack`].
    ///
    /// Defaults to [`ContentFit::Contain`]
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
    images: &Vec<StackedImage<H>>,
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
    images: &Vec<StackedImage<H>>,
    content_fit: ContentFit,
) where
    R: image::Renderer<Handle = H>,
    H: Clone + Hash,
{
    // Find out maximum size (assuming the first image covers the entire area)
    let Size { width, height } = renderer.dimensions(&images[0].handle);
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

            let pos_offset = Vector::new((image.x as f32) * x_scale, (image.y as f32) * y_scale);

            let drawing_bounds = Rectangle {
                width: width as f32 * x_scale,
                height: height as f32 * y_scale,
                ..bounds
            };

            let sum_bounds = drawing_bounds + center_offset + pos_offset;

            //println!("bounds {:?}", sum_bounds);

            (image, sum_bounds)
        })
        .collect();

    let render = |renderer: &mut R| {
        for (image, bounds) in to_draw.iter() {
            renderer.draw(image.handle.clone(), *bounds);
        }
    };

    if overall_adjusted_fit.width > bounds.width || overall_adjusted_fit.height > bounds.height {
        renderer.with_layer(bounds, render);
    } else {
        render(renderer)
    }
}

impl<M, R, H> Widget<M, R> for ImageStack<H>
where
    R: image::Renderer<Handle = H>,
    H: Clone + Hash,
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
        _state: &Tree,
        renderer: &mut R,
        _theme: &R::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        draw(renderer, layout, &self.images, self.content_fit)
    }
}

impl<'a, M, R, H> From<ImageStack<H>> for Element<'a, M, R>
where
    R: image::Renderer<Handle = H>,
    H: Clone + Hash + 'a,
{
    fn from(image: ImageStack<H>) -> Element<'a, M, R> {
        Element::new(image)
    }
}
