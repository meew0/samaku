pub use iced::widget::pane_grid::Pane;

use crate::message;

pub mod grid;
pub mod node_editor;
pub mod style_editor;
pub mod text_editor;
pub mod unassigned;
pub mod video;

/// The state information contained by a pane: what type of pane it is, as well as any
/// extra data that is specific to the pane itself, like the state of control elements.
pub struct State {
    pub local: Box<dyn LocalState>,
}

impl State {
    #[must_use]
    pub fn new(local: Box<dyn LocalState>) -> Self {
        Self { local }
    }

    #[must_use]
    pub fn unassigned() -> Self {
        Self::new(Box::new(unassigned::State {}))
    }
}

pub trait LocalState {
    fn view<'a>(&'a self, self_pane: Pane, global_state: &'a crate::Samaku) -> View<'a>;

    fn update(&mut self, _pane_message: message::Pane) -> iced::Task<message::Message> {
        iced::Task::none()
    }

    fn visit(&mut self, _visitor: &dyn Visitor) {}

    fn update_filter_names(&mut self, _extradata: &crate::subtitle::Extradata) {}
    fn update_style_lists(
        &mut self,
        _styles: &[crate::subtitle::Style],
        _copy_styles: bool,
        _active_event_style_index: Option<usize>,
    ) {
    }
}

/// Visitor pattern implementation for local state types that potentially need custom pane-specific global update behavior
///
/// For instance, the node editor pane needs to be accessible for the global update method because certain messages
/// require the global update method to change some details about the pane state. For this purpose, a type implementing
/// this trait can be passed to the `LocalState::visit` method, which will result in the `visit_node_editor` method
/// being called only for node editor panes.
pub trait Visitor {
    fn visit_node_editor(&self, _node_editor_state: &mut node_editor::State) {}
}

pub type Constructor = fn() -> Box<dyn LocalState>;

/// An empty “shell” of a node that can be used to create a pane later on.
///
/// Represents the idea of a pane, with any specific type information being erased. We use the
/// `inventory` crate to collect pane shells, to be able to iterate over them in all places where
/// we need a list of registered panes (like on the unassigned pane)
#[derive(Debug, Clone)]
pub struct Shell {
    pub name: &'static str,
    pub constructor: Constructor,
}

impl Shell {
    pub const fn new(name: &'static str, constructor: Constructor) -> Self {
        Self { name, constructor }
    }
}

inventory::collect!(Shell);

/// Struct containing the elements to be shown in a pane and in its title bar, for use as the return
/// type of each pane's `view` function.
pub struct View<'a> {
    pub title: iced::Element<'a, message::Message>,
    pub content: iced::Element<'a, message::Message>,
}
