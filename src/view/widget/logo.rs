//! The samaku logo, drawn directly onto an `iced` canvas.
//!
//! This is a port of the logo SVG file found at `src/resources/logo.svg`,
//! so that we don't need to depend on a bunch of SVG-related crates just to draw one image.
//! The path data is specified in the SVG file's units, to keep the two comparable for the future.

use iced::advanced::Renderer as _;
use iced::advanced::graphics::geometry::Renderer as _;
use iced::advanced::widget::{Tree, tree};
use iced::advanced::{Layout, Widget, layout, mouse, renderer};
use iced::widget::canvas;

use crate::style;

/// The `viewBox` of the original SVG, which the logo is laid out in and scaled from.
const VIEW_BOX: iced::Size = iced::Size::new(115.0, 130.0);

/// The `transform="translate(...)"` on the SVG's single layer, rounded to whole units.
const LAYER_OFFSET: iced::Vector = iced::Vector::new(2.0, 2.0);

/// The stroke width shared by the ring and the thick swoosh.
const THICK_STROKE: f32 = 10.0;

/// The stroke width of the thin swoosh.
const THIN_STROKE: f32 = 6.0;

/// A [`canvas::Program`] that draws the samaku logo,
/// scaled to fit its bounds while preserving the aspect ratio.
#[derive(Debug, Default)]
pub struct Logo;

impl<Message> canvas::Program<Message> for Logo {
    /// The geometry never changes, so it only needs to be tessellated when the size does.
    type State = canvas::Cache;

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: iced::Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        vec![state.draw(renderer, bounds.size(), |frame| {
            // Fit the view box into the available space like `ContentFit::Contain` would,
            // which is what the `svg` widget would do, and center what is left over.
            let scale = (frame.width() / VIEW_BOX.width).min(frame.height() / VIEW_BOX.height);
            frame.translate(iced::Vector::new(
                VIEW_BOX.width.mul_add(-scale, frame.width()) / 2.0,
                VIEW_BOX.height.mul_add(-scale, frame.height()) / 2.0,
            ));
            frame.scale(scale);
            frame.translate(LAYER_OFFSET);

            // Everything below is drawn in the SVG's user units. Note that `Frame` transforms the
            // path before tessellating it but leaves the stroke width alone, so stroke widths have
            // to be scaled by hand.
            let stroke = |width: f32| {
                canvas::Stroke::default()
                    .with_color(style::SAMAKU_PRIMARY)
                    .with_width(width * scale)
                    .with_line_cap(canvas::LineCap::Butt)
                    .with_line_join(canvas::LineJoin::Miter)
            };

            // `<circle cx="55.500004" cy="69.787514" r="46.999996" />`, stroked but not filled.
            frame.stroke(
                &canvas::Path::new(|builder| {
                    builder.circle(iced::Point::new(55.500_004, 69.787_51), 46.999_996);
                }),
                stroke(THICK_STROKE),
            );

            // The long swoosh sweeping up and to the right, `path2` in the SVG.
            frame.stroke(
                &canvas::Path::new(|builder| {
                    builder.move_to(iced::Point::new(14.617_683, 92.974_48));
                    builder.bezier_curve_to(
                        iced::Point::new(14.617_683, 92.974_48),
                        iced::Point::new(39.687_01, 85.991_99),
                        iced::Point::new(51.713_696, 74.134_81),
                    );
                    builder.bezier_curve_to(
                        iced::Point::new(65.065_09, 60.971_61),
                        iced::Point::new(71.486_02, 45.373_165),
                        iced::Point::new(77.842_57, 28.437_672),
                    );
                }),
                stroke(THICK_STROKE),
            );

            // The short swoosh curving down to the right, `path3` in the SVG.
            frame.stroke(
                &canvas::Path::new(|builder| {
                    builder.move_to(iced::Point::new(51.713_696, 74.134_81));
                    builder.bezier_curve_to(
                        iced::Point::new(82.648_89, 77.359_05),
                        iced::Point::new(95.737_48, 94.076_3),
                        iced::Point::new(95.737_48, 94.076_3),
                    );
                }),
                stroke(THIN_STROKE),
            );

            // The solid crescent filling the bottom of the ring, `path4` in the SVG.
            frame.fill(
                &canvas::Path::new(|builder| {
                    builder.move_to(iced::Point::new(95.741_77, 94.075_935));
                    builder.bezier_curve_to(
                        iced::Point::new(87.513_36, 107.688_21),
                        iced::Point::new(72.569_31, 116.787_59),
                        iced::Point::new(55.500_004, 116.787_59),
                    );
                    builder.bezier_curve_to(
                        iced::Point::new(38.011_55, 116.787_59),
                        iced::Point::new(22.754_019, 107.235_85),
                        iced::Point::new(14.659_687, 93.064_69),
                    );
                    builder.bezier_curve_to(
                        iced::Point::new(14.659_687, 93.064_69),
                        iced::Point::new(39.687_01, 85.991_99),
                        iced::Point::new(51.713_696, 74.134_81),
                    );
                    builder.bezier_curve_to(
                        iced::Point::new(82.648_89, 77.359_05),
                        iced::Point::new(95.741_77, 94.075_935),
                        iced::Point::new(95.741_77, 94.075_935),
                    );
                    builder.close();
                }),
                style::SAMAKU_PRIMARY,
            );
        })]
    }
}

/// A widget drawing the samaku logo at a fixed square size.
///
/// This exists instead of a plain [`canvas`] widget because the geometry has to end up in a render
/// layer of its own: with antialiasing enabled, `iced_wgpu` renders all the meshes of one layer
/// into a shared multisampled texture, and only the region covered by the scissor rectangle of the
/// *last* mesh it drew survives being resolved back into the frame. The logo is drawn long before
/// the timeline pane's canvas, so within a shared layer it would be discarded. Pushing a layer
/// gives it its own multisampling pass.
#[derive(Debug)]
struct LogoWidget {
    size: f32,
}

impl<Message> Widget<Message, iced::Theme, iced::Renderer> for LogoWidget {
    fn size(&self) -> iced::Size<iced::Length> {
        iced::Size {
            width: iced::Length::Fixed(self.size),
            height: iced::Length::Fixed(self.size),
        }
    }

    fn layout(
        &mut self,
        _tree: &mut Tree,
        _renderer: &iced::Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        layout::atomic(
            limits,
            iced::Length::Fixed(self.size),
            iced::Length::Fixed(self.size),
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut iced::Renderer,
        theme: &iced::Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _viewport: &iced::Rectangle,
    ) {
        let bounds = layout.bounds();

        if bounds.width < 1.0 || bounds.height < 1.0 {
            return;
        }

        let state = tree.state.downcast_ref::<canvas::Cache>();

        renderer.with_layer(bounds, |layer_renderer| {
            layer_renderer.with_translation(
                iced::Vector::new(bounds.x, bounds.y),
                |translated_renderer| {
                    for geometry in canvas::Program::<Message>::draw(
                        &Logo,
                        state,
                        translated_renderer,
                        theme,
                        bounds,
                        cursor,
                    ) {
                        translated_renderer.draw_geometry(geometry);
                    }
                },
            );
        });
    }

    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<canvas::Cache>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(canvas::Cache::<iced::Renderer>::default())
    }
}

impl<'a, Message: 'a> From<LogoWidget> for iced::Element<'a, Message> {
    fn from(widget: LogoWidget) -> Self {
        Self::new(widget)
    }
}

/// Creates a widget drawing the samaku logo at the given square size.
#[must_use]
pub fn logo<'a, Message: 'a>(size: f32) -> iced::Element<'a, Message> {
    LogoWidget { size }.into()
}
