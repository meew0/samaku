// samaku's codebase makes heavy use of optional lints to enforce a “standard” code style.
// The aim is to warn for *all* lints, however in practice, some lints conflict with each other
// and in some cases the cost is too large for the benefit. In borderline cases, the lints are
// explicitly specified with `allow` together with a reason to explain why we chose not to
// warn for this particular lint. But in cases where it is mostly obvious why we would not want
// a particular lint, the lint is not specified at all.
// Some lints are unstable but desirable once they are stabilized; these are commented out
// for now.
//
// --- clippy lint groups ---
#![warn(clippy::pedantic)]
#![warn(clippy::style)]
//
// --- clippy individual restriction/nursery lints ---
// #![warn(clippy::absolute_paths)]
// #![warn(clippy::alloc_instead_of_core)]
#![warn(clippy::allow_attributes)]
#![warn(clippy::allow_attributes_without_reason)]
#![allow(
    clippy::arbitrary_source_item_ordering,
    reason = "potentially warn in the future; currently unclear which ordering should be used exactly in which cases"
)]
#![allow(
    clippy::arithmetic_side_effects,
    reason = "potentially warn in the future; explicit side effect checking makes arithmetic code much less clean, and I'm not sure yet whether it would be worth the slight correctness/safety benefit"
)]
// #![warn(clippy::as_conversions)]
#![warn(clippy::as_pointer_underscore)]
#![warn(clippy::as_ptr_cast_mut)]
#![warn(clippy::as_underscore)]
#![warn(clippy::assertions_on_result_states)]
// #![warn(clippy::big_endian_bytes)]
#![warn(clippy::branches_sharing_code)]
#![warn(clippy::cargo_common_metadata)]
#![warn(clippy::cfg_not_test)]
#![warn(clippy::clear_with_drain)]
#![warn(clippy::clone_on_ref_ptr)]
#![warn(clippy::coerce_container_to_any)]
#![warn(clippy::cognitive_complexity)]
#![warn(clippy::collection_is_never_read)]
#![warn(clippy::create_dir)]
#![allow(
    clippy::dbg_macro,
    reason = "while samaku is in pre-alpha state, it's fine to have debug print statements scattered over the code"
)]
#![warn(clippy::debug_assert_with_mut_call)]
#![warn(clippy::decimal_literal_representation)]
#![allow(
    clippy::default_numeric_fallback,
    reason = "useful but far too many false positives"
)]
#![warn(clippy::default_union_representation)]
#![warn(clippy::deref_by_slicing)]
#![warn(clippy::derive_partial_eq_without_eq)]
// #![warn(clippy::disallowed_script_idents)]
#![warn(clippy::doc_include_without_cfg)]
#![warn(clippy::doc_link_code)]
#![warn(clippy::doc_paragraphs_missing_punctuation)]
#![allow(clippy::else_if_without_else, reason = "add in the future")]
#![warn(clippy::empty_drop)]
#![warn(clippy::empty_enum_variants_with_brackets)]
#![warn(clippy::empty_structs_with_brackets)]
#![warn(clippy::equatable_if_let)]
#![warn(clippy::error_impl_error)]
// #![warn(clippy::exhaustive_enums)]
// #![warn(clippy::exhaustive_structs)]
#![warn(clippy::exit)]
// #![warn(clippy::expect_used)]
#![warn(clippy::fallible_impl_from)]
#![warn(clippy::field_scoped_visibility_modifiers)]
#![warn(clippy::filetype_is_file)]
// #![warn(clippy::float_arithmetic)]
#![warn(clippy::float_cmp_const)]
#![warn(clippy::fn_to_numeric_cast_any)]
#![warn(clippy::future_not_send)]
#![warn(clippy::get_unwrap)]
#![warn(clippy::host_endian_bytes)]
#![warn(clippy::if_then_some_else_none)]
#![warn(clippy::impl_trait_in_params)]
// #![warn(clippy::implicit_return)]
#![warn(clippy::imprecise_flops)]
// #![warn(clippy::indexing_slicing)]
#![warn(clippy::infinite_loop)]
// #![warn(clippy::inline_asm_x86_att_syntax)]
// #![warn(clippy::inline_asm_x86_intel_syntax)]
// #![warn(clippy::integer_division)]
// #![warn(clippy::integer_division_remainder_used)]
#![warn(clippy::iter_on_empty_collections)]
#![warn(clippy::iter_on_single_items)]
#![allow(
    clippy::iter_over_hash_type,
    reason = "potentially warn in the future; might require some refactoring"
)]
#![warn(clippy::iter_with_drain)]
#![warn(clippy::large_stack_frames)]
#![warn(clippy::let_underscore_must_use)]
#![warn(clippy::let_underscore_untyped)]
#![warn(clippy::literal_string_with_formatting_args)]
// #![warn(clippy::little_endian_bytes)]
#![warn(clippy::lossy_float_literal)]
#![allow(clippy::map_err_ignore, reason = "add in the future")]
#![warn(clippy::map_with_unused_argument_over_ranges)]
#![warn(clippy::mem_forget)]
#![warn(clippy::min_ident_chars)]
#![allow(clippy::missing_assert_message, reason = "add in the future")]
#![warn(clippy::missing_asserts_for_indexing)]
// #![warn(clippy::missing_const_for_fn)]
#![allow(
    clippy::missing_docs_in_private_items,
    reason = "potentially warn in the future (together with `missing_docs`); it would be nice if all of the code was documented, but that requires a lot of effort"
)]
// #![warn(clippy::missing_inline_in_public_items)]
// #![warn(clippy::missing_trait_methods)]
#![warn(clippy::mixed_read_write_in_expression)]
// #![warn(clippy::mod_module_files)]
#![warn(clippy::module_name_repetitions)]
// #![warn(clippy::module_arithmetic)]
#![allow(
    clippy::multiple_crate_versions,
    reason = "potentially warn in the future; needs careful balancing of dependency versions"
)]
#![warn(clippy::multiple_inherent_impl)]
// #![warn(clippy::multiple_unsafe_ops_per_block)]
#![warn(clippy::mutex_atomic)]
#![warn(clippy::mutex_integer)]
#![warn(clippy::needless_collect)]
#![warn(clippy::needless_pass_by_ref_mut)]
#![warn(clippy::needless_raw_strings)]
#![warn(clippy::needless_type_cast)]
#![warn(clippy::negative_feature_names)]
// #![warn(clippy::non_ascii_literal)]
#![allow(clippy::non_send_fields_in_send_ty, reason = "add in the future")]
#![warn(clippy::non_zero_suggestions)]
#![warn(clippy::nonstandard_macro_braces)]
// #![warn(clippy::option_if_let_else)]
#![warn(clippy::or_fun_call)]
// #![warn(clippy::panic)]
// #![warn(clippy::panic_in_result_fn)]
#![allow(
    clippy::partial_pub_fields,
    reason = "potentially warn in the future; requires some refactoring"
)]
#![warn(clippy::path_buf_push_overwrite)]
#![warn(clippy::pathbuf_init_then_push)]
#![allow(clippy::pattern_type_mismatch, reason = "add in the future")]
#![allow(
    clippy::pointer_format,
    reason = "we have no reason to keep addresses private"
)]
#![warn(clippy::precedence_bits)]
#![allow(
    clippy::print_stderr,
    reason = "while samaku is in pre-alpha state, it's fine to have debug print statements scattered over the code"
)]
#![allow(
    clippy::print_stdout,
    reason = "while samaku is in pre-alpha state, it's fine to have debug print statements scattered over the code"
)]
// #![warn(clippy::pub_use)]
// #![warn(clippy::pub_with_shorthand)]
#![warn(clippy::pub_without_shorthand)]
// #![warn(clippy::question_mark_used)]
#![warn(clippy::rc_buffer)]
#![warn(clippy::rc_mutex)]
#![warn(clippy::read_zero_byte_vec)]
#![warn(clippy::redundant_clone)]
// #![warn(clippy::redundant_feature_names)]
#![allow(clippy::redundant_pub_crate, reason = "add in the future")]
#![warn(clippy::redundant_test_prefix)]
#![warn(clippy::redundant_type_annotations)]
// #![warn(clippy::ref_patterns)]
#![warn(clippy::renamed_function_params)]
#![warn(clippy::rest_pat_in_fully_bound_structs)]
#![warn(clippy::return_and_then)]
#![warn(clippy::same_name_method)]
#![warn(clippy::search_is_some)]
#![warn(clippy::self_named_module_files)]
#![warn(clippy::semicolon_inside_block)]
// #![warn(clippy::semicolon_outside_block)]
// #![warn(clippy::separated_literal_suffix)]
#![warn(clippy::set_contains_or_insert)]
#![allow(clippy::shadow_reuse, reason = "add in the future")]
#![allow(clippy::shadow_same, reason = "add in the future")]
#![allow(clippy::shadow_unrelated, reason = "add in the future")]
#![warn(clippy::significant_drop_in_scrutinee)]
#![warn(clippy::significant_drop_tightening)]
// #![warn(clippy::single_call_fn)]
// #![warn(clippy::single_char_lifetime_names)]
#![warn(clippy::single_option_map)]
// #![warn(clippy::std_instead_of_alloc)]
// #![warn(clippy::std_instead_of_core)]
#![warn(clippy::str_to_string)]
#![warn(clippy::string_add)]
#![warn(clippy::string_lit_as_bytes)]
#![warn(clippy::string_lit_chars_any)]
#![allow(clippy::string_slice, reason = "add in the future")]
#![warn(clippy::suboptimal_flops)]
#![warn(clippy::suspicious_operation_groupings)]
#![warn(clippy::suspicious_xor_used_as_pow)]
#![warn(clippy::tests_outside_test_module)]
#![allow(clippy::todo, reason = "allowed while samaku is in pre-alpha")]
#![warn(clippy::too_long_first_doc_paragraph)]
#![warn(clippy::trailing_empty_array)]
#![warn(clippy::trait_duplication_in_bounds)]
#![warn(clippy::transmute_undefined_repr)]
#![warn(clippy::trivial_regex)]
#![warn(clippy::try_err)]
#![warn(clippy::tuple_array_conversions)]
#![warn(clippy::type_repetition_in_bounds)]
#![allow(
    clippy::undocumented_unsafe_blocks,
    reason = "potentially warn in the future; most of our unsafe code is FFI-related and it is hard to properly document the safety of that"
)]
#![allow(clippy::unimplemented, reason = "allowed while samaku is in pre-alpha")]
#![warn(clippy::uninhabited_references)]
#![warn(clippy::unnecessary_safety_comment)]
#![warn(clippy::unnecessary_safety_doc)]
#![warn(clippy::unnecessary_self_imports)]
#![warn(clippy::unnecessary_struct_initialization)]
#![warn(clippy::unneeded_field_pattern)]
// #![warn(clippy::unreachable)]
#![warn(clippy::unseparated_literal_suffix)]
#![warn(clippy::unused_peekable)]
#![warn(clippy::unused_result_ok)]
#![warn(clippy::unused_rounding)]
#![warn(clippy::unused_trait_names)]
// #![warn(clippy::unwrap_in_result)]
#![allow(
    clippy::unwrap_used,
    reason = "potentially warn in the future; requires some refactoring, but would improve error message in unexpected cases"
)]
// #![warn(clippy::use_debug)]
// #![warn(clippy::use_self)] // we would ideally want the opposite of this
#![warn(clippy::useless_let_if_seq)]
#![warn(clippy::verbose_file_reads)]
#![warn(clippy::volatile_composites)]
#![warn(clippy::while_float)]
#![warn(clippy::wildcard_dependencies)]
#![allow(
    clippy::wildcard_enum_match_arm,
    reason = "potentially warn in the future; a bit too noisy right now"
)]
//
// --- builtin lints ---
// Note: some obvious ones (primarily lints applying to past editions of Rust) are skipped.
#![warn(absolute_paths_not_starting_with_crate)]
#![warn(ambiguous_negative_literals)]
#![warn(closure_returning_async_block)]
#![warn(deref_into_dyn_supertrait)]
#![allow(
    elided_lifetimes_in_paths,
    reason = "potentially warn in the future; too noisy for now"
)]
#![warn(explicit_outlives_requirements)]
// #![warn(fuzzy_provenance_casts)] // add once stable
#![warn(if_let_rescope)]
#![warn(impl_trait_overcaptures)]
#![warn(impl_trait_redundant_captures)]
#![warn(let_underscore_drop)]
// #![warn(lossy_provenance_casts)] // add once stable
#![warn(macro_use_extern_crate)]
#![warn(meta_variable_misuse)]
#![warn(missing_abi)]
#![allow(
    missing_docs,
    reason = "potentially warn in the future; it would be nice if all of the code was documented, but that requires a lot of effort"
)]
#![warn(missing_unsafe_on_extern)]
// #![warn(multiple_supertrait_upcastable)] // add once stable
// #![warn(must_not_suspend)] // add once stable
// #![warn(non_exhaustive_omitted_patterns)] // add once stable
#![warn(redundant_imports)]
#![warn(redundant_lifetimes)]
// #![warn(resolving_to_items_shadowing_supertrait_items)] // add once stable
// #![warn(shadowing_supertrait_items)] // add once stable
#![warn(single_use_lifetimes)]
#![warn(trivial_casts)]
#![warn(trivial_numeric_casts)]
#![warn(unit_bindings)]
#![allow(
    unnameable_types,
    reason = "potentially warn in the future; to be determined how much work this requires"
)]
// #![warn(unqualified_local_imports)] // add once stable
#![warn(unreachable_pub)]
#![warn(unsafe_attr_outside_unsafe)]
#![allow(
    unsafe_code,
    reason = "samaku is not unsafe-free; we need unsafe to interface with C code, and also, it's fine to use unsafe in limited cases where we can prove that rustc is unnecessarily restrictive"
)]
#![warn(unsafe_op_in_unsafe_fn)]
#![warn(unused_crate_dependencies)]
#![warn(unused_extern_crates)]
#![warn(unused_import_braces)]
#![warn(unused_lifetimes)]
#![warn(unused_macro_rules)]
#![warn(unused_qualifications)]
#![allow(
    unused_results,
    reason = "too sensitive; `must_use` detection is enough"
)]
#![allow(
    variant_size_differences,
    reason = "potentially useful, but the 3x rule is far too sensitive and cannot be configured"
)]
//
// --- warn-/deny-by-default lints that we want to allow ---
#![allow(
    clippy::doc_markdown,
    reason = "false positives on any kind of camel case-looking words"
)]
#![allow(
    clippy::enum_glob_use,
    reason = "too useful to disallow entirely, but should only be done locally"
)]
#![allow(
    clippy::missing_errors_doc,
    reason = "not so useful in application code where results are widely used for error handling without usually being able to delineate specific circumstances"
)]
#![allow(
    clippy::struct_field_names,
    reason = "https://github.com/rust-lang/rust-clippy/issues/12922#issuecomment-2166124359"
)]
//
// --- these lints we only want to allow in test code ---
#![cfg_attr(
    test,
    allow(
        clippy::cognitive_complexity,
        reason = "it doesn't really matter if test functions are complex"
    )
)]
#![cfg_attr(
    test,
    allow(
        clippy::too_many_lines,
        reason = "it doesn't really matter if test functions are complex"
    )
)]

// These following 3 crates are only used in benchmarks.
// We need to import them here to suppress the relevant clippy warnings.
#[cfg(test)]
#[expect(
    clippy::useless_attribute,
    reason = "not actually useless in this case, lint false positive"
)]
#[expect(
    clippy::allow_attributes,
    reason = "allow specifically needed to prevent IDE from automatically deleting 'unused' import"
)]
#[allow(unused_imports, reason = "only used in benchmarks")]
use criterion as _;
#[cfg(test)]
#[expect(
    clippy::useless_attribute,
    reason = "not actually useless in this case, lint false positive"
)]
#[expect(
    clippy::allow_attributes,
    reason = "allow specifically needed to prevent IDE from automatically deleting 'unused' import"
)]
#[allow(unused_imports, reason = "only used in benchmarks")]
use rand as _;
#[cfg(test)]
#[expect(
    clippy::useless_attribute,
    reason = "not actually useless in this case, lint false positive"
)]
#[expect(
    clippy::allow_attributes,
    reason = "allow specifically needed to prevent IDE from automatically deleting 'unused' import"
)]
#[allow(unused_imports, reason = "only used in benchmarks")]
use rand_pcg as _;

use std::cell::RefCell;
use std::sync::{Arc, Mutex};

use iced::widget::container;
use iced::widget::pane_grid::{self, PaneGrid};
use iced::{Alignment, Event};
use iced::{Element, Length, Settings, Subscription};

mod action;
pub mod config;
pub mod history;
pub mod keyboard;
pub mod media;
pub mod menu;
pub mod message;
pub mod model;
pub mod nde;
pub mod pane;
pub mod project;
pub mod resources;
pub mod style;
pub mod subtitle;
mod update;
pub mod version;
pub mod view;
pub mod workers;

/// Effectively samaku's main function. Creates and starts the application.
#[expect(
    clippy::missing_errors_doc,
    reason = "main function doesn't need error documentation"
)]
pub fn run() -> iced::Result {
    iced::application(Samaku::boot, update::update, Samaku::view)
        .subscription(Samaku::subscription)
        .settings(Settings {
            id: Some("samaku".to_owned()),
            fonts: vec![
                resources::BARLOW.into(),
                iced_fonts::BOOTSTRAP_FONT_BYTES.into(),
            ],
            default_font: DEFAULT_FONT,
            default_text_size: iced::Pixels(16.0),
            antialiasing: true,
            vsync: true,
        })
        .window_size(iced::Size::new(1600.0, 1000.0))
        .title(title)
        .theme(theme)
        .run()
}

pub const DEFAULT_FONT: iced::Font = iced::Font {
    family: iced::font::Family::Name("Barlow"),
    weight: iced::font::Weight::Normal,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

/// Global application state.
pub struct Samaku {
    /// Workers represent separate threads running certain CPU-intensive tasks, like video and audio
    /// decoding. The `Workers` interface is available to send messages to them.
    workers: workers::Workers,

    /// State that needs to be shared with the workers, like the playback position.
    shared: SharedState,

    /// State that needs to be mutable in view code, like caching of results to avoid rerunning
    /// certain calculations over and over.
    view: RefCell<ViewState>,

    /// The history (undo/redo) tree, containing previous states that can be returned to
    /// by undo and redo.
    history: history::History,

    /// Currently pressed keyboard modifiers.
    modifiers: iced::keyboard::Modifiers,

    /// The current state of the global pane grid.
    /// Includes all state for the individual panes themselves.
    panes: pane_grid::State<pane::State>,

    /// Currently focused pane, if one exists.
    focus: Option<pane_grid::Pane>,

    /// Toasts (notifications) to be shown over the UI.
    pub toasts: model::toast::List<message::Message>,

    /// Metadata of the currently loaded video, if and only if any is loaded.
    pub video_metadata: Option<media::VideoMetadata>,

    /// Currently loaded subtitles. Will contain some useful defaults if nothing has been loaded
    /// yet.
    pub subtitles: subtitle::File,

    /// The set of events, identified by index, that are currently selected.
    pub selected_events: model::select::EventSelection,

    /// Project properties, that is, data not stored elsewhere that conceptually belongs to a “Samaku project”,
    /// for example, paths to linked media files etc.
    pub project_properties: project::Properties,

    /// The number of the frame that is actually being displayed right now,
    /// together with the image it represents.
    /// Will be slightly different from the information in
    /// `playback_state` due to decoding latency etc.
    pub actual_frame: Option<(model::FrameNumber, iced::widget::image::Handle)>,

    /// Our own representation of whether playback is currently running or not.
    /// Setting this does nothing; it is updated by playback controller workers.
    pub playing: bool,

    /// Control widgets that are shown over the video, in order to allow quick setting of positions
    /// and the like.
    pub reticules: Option<model::reticule::Reticules>,
}

/// Data that needs to be shared with workers.
pub struct SharedState {
    /// Currently loaded audio, if present.
    /// Can be shared into workers etc., but be sure not to hold the mutex for
    /// too long, otherwise the playback worker will stall.
    pub audio: Arc<Mutex<Option<media::Audio>>>,

    /// Authoritative playback position and state.
    /// Set this to seek/pause/resume etc.
    pub playback_position: Arc<model::playback::Position>,
}

/// More-or-less temporary data, that needs to be mutable within View functions.
pub struct ViewState {
    pub subtitle_renderer: media::subtitle::Renderer,
}

/// Utility methods for global state.
impl Samaku {
    /// Returns the frame rate of the loaded video, or 24 fps if no video is loaded.
    pub fn frame_rate(&self) -> media::FrameRate {
        if let Some(video_metadata) = self.video_metadata {
            video_metadata.frame_rate
        } else {
            media::FrameRate {
                numerator: 24,
                denominator: 1,
            }
        }
    }

    /// Create a context for compilation.
    pub fn compile_context(&self) -> subtitle::compile::Context {
        subtitle::compile::Context {
            frame_rate: self.frame_rate(),
        }
    }

    /// Get the best guess for the number of the currently displayed frame. Returns `None` if no
    /// video is loaded.
    pub fn current_frame(&self) -> Option<model::FrameNumber> {
        match self.actual_frame {
            Some((frame, _)) => Some(frame),
            None => self.video_metadata.map(|metadata| {
                self.shared
                    .playback_position
                    .current_frame(metadata.frame_rate)
            }),
        }
    }

    fn boot() -> (Self, iced::Task<message::Message>) {
        (Samaku::default(), iced::Task::none())
    }

    /// Construct the user interface. Called whenever iced needs to rerender the application.
    fn view(&'_ self) -> Element<'_, message::Message> {
        let focus = self.focus;

        // The pane grid makes up the main part of the application. All the fundamental
        // functionality, like moving panes around, is provided by iced here; we just take care
        // of filling the panes with content.
        let pane_grid =
            PaneGrid::new::<pane::State>(&self.panes, |pane, pane_state, _is_maximized| {
                // This closure is called for every pane.

                let is_focused = focus == Some(pane);

                // Construct the user interface within the pane itself, based on whatever the pane
                // struct wants to do.
                let pane_view = pane_state.local.view(pane, self);
                let title_bar =
                    pane_grid::TitleBar::new(pane_view.title)
                        .padding(5)
                        .style(if is_focused {
                            style::title_bar_focused
                        } else {
                            style::title_bar_active
                        });
                pane_grid::Content::new(pane_view.content)
                    .title_bar(title_bar)
                    .style(if is_focused {
                        style::pane_focused
                    } else {
                        style::pane_active
                    })
            })
            .width(Length::Fill)
            .height(Length::Fill)
            .spacing(5)
            .on_click(message::Message::FocusPane)
            .on_drag(message::Message::DragPane)
            .on_resize(0, message::Message::ResizePane);

        // We implement our own non-native menu using iced_aw. The entry definitions are located
        // in `menu.rs`.
        // Once iced supports native menus again, we may switch to that.
        let menu_bar = iced_aw::menu::MenuBar::new(vec![
            menu::file(),
            menu::edit(&self.history),
            menu::media(),
        ])
        .spacing(5.0)
        .width(180)
        .height(32);

        // The title row — currently only contains the logo and the application name.
        let title_row = iced::widget::row![
            iced::widget::svg(iced::widget::svg::Handle::from_memory(resources::LOGO))
                .width(30)
                .height(30),
            iced::widget::text("samaku")
                .size(25)
                .style(|_theme| iced::widget::text::Style {
                    color: Some(style::SAMAKU_PRIMARY)
                }),
            iced::widget::Space::new().width(Length::Fixed(10.0)),
            menu_bar
        ]
        .spacing(5)
        .align_y(Alignment::Center);

        let content: Element<message::Message> =
            container(iced::widget::column![title_row, pane_grid].spacing(10))
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(5)
                .into();

        view::toast::Manager::new(
            content,
            self.toasts.as_slice(),
            message::Message::CloseToast,
        )
        .timeout(view::toast::DEFAULT_TIMEOUT)
        .into()
    }

    fn subscription(&self) -> Subscription<message::Message> {
        use iced::advanced::graphics::futures::{MaybeSend, boxed_stream};
        use iced::advanced::subscription::{EventStream, Recipe};
        use iced::futures::StreamExt as _;
        use std::hash::Hasher as _;

        // Basically this reimplements a simplified version of `iced::subscription::Runner` which is private.
        struct StreamListener<S, T>
        where
            S: iced::futures::Stream<Item = T> + MaybeSend + 'static,
        {
            stream: S,
        }

        impl<S, T> Recipe for StreamListener<S, T>
        where
            S: iced::futures::Stream<Item = T> + MaybeSend + 'static,
        {
            type Output = T;

            fn hash(&self, state: &mut iced::advanced::subscription::Hasher) {
                state.write_u64(0xcafe_babe);
            }

            fn stream(
                self: Box<Self>,
                _input: EventStream,
            ) -> iced::futures::stream::BoxStream<'static, Self::Output> {
                boxed_stream(self.stream)
            }
        }

        // Handle key presses for shortcut purposes
        let shortcut_events = iced::event::listen_with(|event, status, _window_id| {
            if status == iced::event::Status::Captured {
                return None;
            }

            // Call the function in the `keyboard` module for every key press.
            match event {
                Event::Keyboard(iced::keyboard::Event::KeyPressed {
                    key,
                    modifiers,
                    location,
                    ..
                }) => keyboard::handle_shortcut(&key, modifiers, location),
                _ => None,
            }
        });

        // Separate keyboard listener that only listens to modifier keys (for multi-select and the like)
        let modifier_events = iced::event::listen_with(|event, _status, _window_id| match event {
            Event::Keyboard(keyboard_event) => keyboard::handle_modifiers(&keyboard_event),
            _ => None,
        });

        // This is the magic code that allows us to listen to messages emitted by the workers.
        // While `subscription` is called frequently, only the result of the first `unfold` call is actually used,
        // which is the only one where `self.workers.receiver.take()` produces a `Some` value.
        // For all subsequent times `subscription` is called, the second argument will be `None`
        // and would lead to a panic if it were unwrapped within the closure, but the closure is never
        // called because the initially created subscription is never overwritten.
        let runner = StreamListener {
            stream: iced::futures::stream::unfold(
                self.workers.receiver.take(),
                async move |mut receiver| {
                    let message = receiver.as_mut().unwrap().next().await.unwrap();
                    Some((message, receiver))
                },
            ),
        };
        let worker_messages = iced::advanced::subscription::from_recipe(runner);

        Subscription::batch(vec![shortcut_events, modifier_events, worker_messages])
    }
}

impl Default for Samaku {
    fn default() -> Self {
        let panes = pane_grid::State::with_configuration(initial_pane_configuration());

        // Initial shared state...
        let shared_state = SharedState {
            audio: Arc::new(Mutex::new(None)),
            playback_position: Arc::new(model::playback::Position::default()),
        };

        // ...and initial global state
        Samaku {
            panes,
            modifiers: iced::keyboard::Modifiers::empty(),
            focus: None,
            toasts: model::toast::List::new(),
            workers: workers::Workers::spawn_all(&shared_state),
            actual_frame: None,
            video_metadata: None,
            subtitles: subtitle::File::default(),
            selected_events: model::select::EventSelection::default(),
            project_properties: project::Properties::default(),
            shared: shared_state,
            view: RefCell::new(ViewState {
                subtitle_renderer: media::subtitle::Renderer::new(),
            }),
            playing: false,
            reticules: None,
            history: history::History::new(),
        }
    }
}

fn title(_state: &Samaku) -> String {
    format!("samaku {}", version::Long)
}

fn theme(_state: &Samaku) -> iced::Theme {
    style::samaku_theme()
}

fn initial_pane_configuration() -> pane_grid::Configuration<pane::State> {
    let video = pane::State::new(Box::new(pane::video::State {}));
    let node_editor = pane::State::new(Box::new(pane::node_editor::State::default()));
    let subtitle_grid = pane::State::new(Box::new(pane::grid::State::default()));
    let text_editor = pane::State::new(Box::new(pane::text_editor::State::default()));
    let timeline = pane::State::new(Box::new(pane::timeline::State::default()));

    // First row: video & node editor
    let top = pane_grid::Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.5,
        a: Box::new(pane_grid::Configuration::Pane(video)),
        b: Box::new(pane_grid::Configuration::Pane(node_editor)),
    };

    // Second row: subtitle grid & text editor
    let bottom_row_1 = pane_grid::Configuration::Split {
        axis: pane_grid::Axis::Vertical,
        ratio: 0.66,
        a: Box::new(pane_grid::Configuration::Pane(subtitle_grid)),
        b: Box::new(pane_grid::Configuration::Pane(text_editor)),
    };

    // Assemble second row (see above) and third row (timeline)
    let bottom = pane_grid::Configuration::Split {
        axis: pane_grid::Axis::Horizontal,
        ratio: 0.5,
        a: Box::new(bottom_row_1),
        b: Box::new(pane_grid::Configuration::Pane(timeline)),
    };

    // Assemble layout
    pane_grid::Configuration::Split {
        axis: pane_grid::Axis::Horizontal,
        ratio: 0.6, // with this, 16:9 videos fit into the video pane
        a: Box::new(top),
        b: Box::new(bottom),
    }
}

#[cfg(test)]
pub mod test_utils {
    use std::env;
    use std::path::{Path, PathBuf};

    /// Creates a `PathBuf` pointing to the given file relative to the root directory, and ensures
    /// the file exists.
    ///
    /// # Panics
    /// Panics if the file could not be found.
    pub fn test_file<P>(join_path: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
        let path = Path::new(&manifest_dir).join(&join_path);
        assert!(
            path.exists(),
            "Could not find test data ({})! Perhaps some relative-path problem?",
            join_path.as_ref().display()
        );
        path
    }
}
