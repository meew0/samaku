use crate::{message, model};
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
pub fn blend_box<
    'a,
    L: model::NamedListIterable + ?Sized,
    Message,
    F: Fn(L::Key) -> Message + 'a,
>(
    state: &'a blend_box::State,
    options: &'a L,
    placeholder: &str,
    selection_opt: Option<model::NamedEntry<L::Key>>,
    on_selected: F,
) -> BlendBox<'a, L, Message, iced::Theme, iced::Renderer> {
    BlendBox::new(state, options, placeholder, selection_opt, on_selected)
}

// Utility functions to create a blend box + CRUD controls
pub struct BlendBoxControls<
    'a,
    K,
    F1: Fn(K) -> message::Message + 'a,
    F2: Fn(K) -> message::Message + 'a,
> {
    pub add_text: &'static str,
    pub add_message: message::Message,
    pub unassign_text: &'static str,
    pub unassign_message: Option<F1>, // if None, unassign button is not shown
    pub delete_text: &'static str,
    pub delete_message: Option<F2>, // if None, delete button is disabled
    pub _phantom: std::marker::PhantomData<&'a K>,
}

pub fn blend_box_controls<
    'a,
    L: model::NamedListIterable + ?Sized + 'static,
    F1: Fn(L::Key) -> message::Message + 'a,
    F2: Fn(L::Key) -> message::Message + 'a,
    F3: Fn(L::Key) -> message::Message + 'a,
>(
    state: &'a blend_box::State,
    options: &'a L,
    placeholder: &str,
    selection_opt: Option<model::NamedEntry<L::Key>>,
    on_selected: F1,
    controls: BlendBoxControls<L::Key, F2, F3>,
) -> iced::Element<'a, message::Message> {
    let mut row = iced::widget::Row::with_capacity(4);

    let blend_box = blend_box(state, options, placeholder, selection_opt, on_selected)
        .width(250.0)
        .icon(blend_box::default_icon());
    row = row.push(blend_box);

    let add_button = super::tooltip(
        super::Icon::Plus.button().on_press(controls.add_message),
        controls.add_text,
    );
    row = row.push(add_button);

    if let Some(unassign_message) = controls.unassign_message {
        let unassign_button_raw = super::Icon::X.button();
        let unassign_button_active = if let Some(selection) = selection_opt {
            unassign_button_raw.on_press(unassign_message(selection.id))
        } else {
            unassign_button_raw
        };
        let unassign_button = super::tooltip(unassign_button_active, controls.unassign_text);
        row = row.push(unassign_button);
    }

    let delete_button_raw = super::Icon::Trash.button();
    let delete_button_active = if let Some(selection) = selection_opt
        && let Some(delete_message) = controls.delete_message
    {
        delete_button_raw.on_press(delete_message(selection.id))
    } else {
        delete_button_raw
    };
    let delete_button = super::tooltip(delete_button_active, controls.delete_text);
    row = row.push(delete_button);

    row.spacing(5.0).into()
}
