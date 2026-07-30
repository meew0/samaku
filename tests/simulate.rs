//! Simulate the entire application and test some basic behaviors.

// TODO apply all the clippy lints to integration tests as well

use samaku::{message, pane, update, view, workers};
use std::borrow::Cow;

fn update<I: Iterator<Item = message::Message>>(messages: I, state: &mut samaku::Samaku) {
    for message in messages {
        let task = update::update(state, message);
        assert_eq!(
            task.units(),
            0,
            "iced tasks not supported in simulator code"
        );
    }
}

struct GridVisitor {
    visited: bool,
}

impl pane::Visitor for GridVisitor {
    fn visit_grid(&mut self, _grid_state: &mut pane::grid::State) {
        self.visited = true;
    }
}

#[test]
fn event_basic() -> Result<(), iced_test::Error> {
    let mut state = samaku::Samaku::new(workers::Workers::spawn_dummies);

    // Create event
    assert!(state.subtitles.events.is_empty());
    let mut simulator = iced_test::simulator(state.view());
    simulator.click(view::Icon::Plus.character().to_string())?;
    update(simulator.into_messages(), &mut state);
    assert_eq!(state.subtitles.events.len(), 1);

    // Select event
    assert!(state.selected_events.is_empty());
    let (_, event) = state.subtitles.events.nth(0);
    let event_time = event.start.format_long().to_string();
    let mut simulator = iced_test::simulator(state.view());
    simulator.click(event_time)?;
    update(simulator.into_messages(), &mut state);
    assert_eq!(state.selected_events.len(), 1);

    // Edit event data
    // TODO: ideally we'd want to edit the event text itself.
    // However, as of iced 0.14 it's not easily possible to directly select/focus
    // a `TextEditor` when simulating.
    let event = state
        .subtitles
        .events
        .active_event_mut(&state.selected_events)
        .unwrap();
    let old_text = "鏖"; // needs to be relatively short since the simulator will click in the middle
    let new_text = "2345";
    event.actor = Cow::Borrowed(old_text);
    update::notify_selected_events(&mut state);
    let mut simulator = iced_test::simulator(state.view());
    simulator.click(old_text)?;
    simulator.typewrite(new_text);
    update(simulator.into_messages(), &mut state);
    let event = state
        .subtitles
        .events
        .active_event_mut(&state.selected_events)
        .unwrap();
    assert_eq!(event.actor, format!("{old_text}{new_text}"));

    Ok(())
}

#[test]
fn panes() -> Result<(), iced_test::Error> {
    let mut state = samaku::Samaku::new(workers::Workers::spawn_dummies);

    // Create event (such that the grid pane is focused)
    assert!(state.subtitles.events.is_empty());
    let mut simulator = iced_test::simulator(state.view());
    simulator.click(view::Icon::Plus.character().to_string())?;
    update(simulator.into_messages(), &mut state);
    assert_eq!(state.subtitles.events.len(), 1);

    // Close grid pane.
    // First, determine if the focused pane is actually the grid pane (it should be)
    let mut grid_visitor = GridVisitor { visited: false };
    state
        .panes
        .get_mut(state.focus.unwrap())
        .unwrap()
        .local
        .visit(&mut grid_visitor);
    assert!(grid_visitor.visited);

    // Close the pane, and check that there is no longer a grid pane afterwards
    update(vec![message::Message::ClosePane].into_iter(), &mut state);
    let mut grid_visitor = GridVisitor { visited: false };
    state
        .panes
        .get_mut(state.focus.unwrap())
        .unwrap()
        .local
        .visit(&mut grid_visitor);
    assert!(!grid_visitor.visited);

    Ok(())
}
