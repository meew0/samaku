pub use blend_box::BlendBox;
pub use image_stack::EmptyProgram;
pub use image_stack::ImageStack;
pub use image_stack::StackedImage;
pub use number_dragger::NumberDragger;

pub mod blend_box;
mod image_stack;
pub mod number_dragger;

pub fn number_dragger<'a, T: number_dragger::Numeric, Message, F: Fn(T) -> Message + 'a>(
    value: T,
    bounds: std::ops::RangeInclusive<T>,
    on_change: F,
) -> NumberDragger<'a, T, Message, iced::Theme, iced::Renderer> {
    NumberDragger::new(value, bounds, on_change)
}
