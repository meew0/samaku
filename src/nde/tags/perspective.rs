//! Convert a screen-space projection quad into override tags.
//!
//! Code mostly derived from https://github.com/arch1t3cht/Aegisub/blob/e79aa896bd676400c2fbbb9e625bc58b491358da/src/visual_tool_perspective.cpp.
//! See https://mz.sb/persp for an explanation of the mathematics behind this transformation.

use crate::nde::BoundingBox;
use crate::nde::tags::{Alignment, Global, Local, Maybe3D, Position, PositionOrMove, Resettable};
use crate::subtitle;
use glam::{DMat2, DMat3, DVec2, DVec3, swizzles::Vec3Swizzles as _};

/// A planar quadrilateral, corners ordered counter-clockwise
/// (`q0` = top-left, `q1` = top-right, `q2` = bottom-right, `q3` = bottom-left;
/// though the math itself only assumes a consistent cyclic order).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(from = "SerdeQuad", into = "SerdeQuad")]
pub struct Quad {
    pub q0: DVec2,
    pub q1: DVec2,
    pub q2: DVec2,
    pub q3: DVec2,
}

impl Quad {
    /// The parameters `(alpha, -beta)` describing where the two diagonals of a quad
    /// intersect, used both for the midpoint and for the convexity check.
    #[must_use]
    pub fn diagonal_intersection(&self) -> (f64, f64) {
        let diag1 = self.q2 - self.q0;
        let diag2 = self.q1 - self.q3;
        let b_vector = self.q3 - self.q0;
        let result_vector = solve_2x2(diag1, diag2, b_vector);
        (result_vector[0], result_vector[1])
    }

    /// Calculates the midpoint of a quad, defined as the intersection of its two diagonals.
    #[must_use]
    pub fn midpoint(&self) -> DVec2 {
        let diag1 = self.q2 - self.q0;
        let (alpha, _) = self.diagonal_intersection();
        self.q0 + alpha * diag1
    }

    /// Check whether this quad is convex, i.e. whether `alpha` and `-beta` are both
    /// between 0 and 1.
    #[inline]
    #[must_use]
    pub fn is_convex(&self) -> bool {
        let (alpha, beta) = self.diagonal_intersection();
        (0.0..=1.0).contains(&alpha) && (0.0..=1.0).contains(&(-beta))
    }

    /// Unwrap a quad into its top-left corner and four vectors relative to it.
    #[inline]
    #[must_use]
    pub fn unwrap(&self) -> (DVec2, DVec2, DVec2, DVec2) {
        (
            self.q0,
            self.q1 - self.q0,
            self.q2 - self.q0,
            self.q3 - self.q0,
        )
    }

    /// Maps a screen-space point `xy` to the quad's bilinear UV coordinates.
    #[must_use]
    pub fn xy_to_uv(&self, point: DVec2) -> DVec2 {
        let (k0, k1, k2, k3) = self.unwrap();
        let point_rel = point - k0;
        let (x1, y1, x2, y2, x3, y3, px, py) =
            (k1.x, k1.y, k2.x, k2.y, k3.x, k3.y, point_rel.x, point_rel.y);

        let uv_u = -((x2.mul_add(y1, -(x1 * y2))
            * x3.mul_add(py, -(px * y3))
            * x1.mul_add(-y2 + y3, x3.mul_add(-y1 + y2, x2 * (y1 - y3))))
            / y2.mul_add(
                (x1 * x1).mul_add(
                    -(px * y3).mul_add(
                        -y2 + y3,
                        (x3 * py).mul_add(2.0_f64.mul_add(-y3, y2), x3 * y2 * y3),
                    ),
                    (px * x3 * x3 * y1).mul_add(
                        y1 - y2,
                        x1 * x3 * x3 * y1.mul_add(y2, py * (-2.0_f64).mul_add(y1, y2)),
                    ),
                ),
                (x2 * x2).mul_add(
                    (x3 * y1 * y1).mul_add(
                        -py + y3,
                        y3 * (px * y1).mul_add(y1 - y3, x1 * (py - y1) * y3),
                    ),
                    x2 * (x1 * y3).mul_add(
                        (x1 * (-py + y2)).mul_add(y3, 2.0 * px * y1 * (-y2 + y3)),
                        (x3 * x3 * y1 * y1).mul_add(
                            py - y2,
                            2.0 * x3 * (x1 * py * y2).mul_add(y1 - y3, px * y1 * (-y1 + y2) * y3),
                        ),
                    ),
                ),
            ));

        let uv_v = (x1.mul_add(py, -(px * y1))
            * x3.mul_add(y2, -(x2 * y3))
            * x2.mul_add(-y1 + y3, x3.mul_add(y1 - y2, x1 * (y2 - y3))))
            / y2.mul_add(
                (x1 * x1).mul_add(
                    (px * y3).mul_add(
                        -y2 + y3,
                        (x3 * py).mul_add(2.0_f64.mul_add(-y3, y2), x3 * y2 * y3),
                    ),
                    (px * x3 * x3 * y1).mul_add(
                        -y1 + y2,
                        x1 * x3 * x3 * y1.mul_add(-y2, (2.0 * py).mul_add(y1, -(py * y2))),
                    ),
                ),
                x2.mul_add(
                    (2.0 * x3).mul_add(
                        -(x1 * py * y2).mul_add(y1 - y3, px * y1 * (-y1 + y2) * y3),
                        (x3 * x3 * y1 * y1).mul_add(
                            -py + y2,
                            x1 * y3 * (2.0 * px * y1).mul_add(y2 - y3, x1 * (py - y2) * y3),
                        ),
                    ),
                    x2 * x2
                        * (x3 * y1 * y1).mul_add(
                            py - y3,
                            y3 * (x1 * (-py + y1)).mul_add(y3, px * y1 * (-y1 + y3)),
                        ),
                ),
            );

        DVec2::new(uv_u, uv_v)
    }

    /// Inverse of [`xy_to_uv`]: maps bilinear `(u, v)` coordinates back to screen-space.
    #[must_use]
    pub fn uv_to_xy(&self, uv: DVec2) -> DVec2 {
        let (k0, k1, k2, k3) = self.unwrap();
        let (x1, y1, x2, y2, x3, y3) = (k1.x, k1.y, k2.x, k2.y, k3.x, k3.y);

        let uv_u = uv.x;
        let uv_v = uv.y;

        let denom = x1.mul_add(
            (-1.0 + uv_u).mul_add(y2, -((-1.0 + uv_u + uv_v) * y3)),
            x3.mul_add(
                uv_v.mul_add(-y2, (-1.0 + uv_u + uv_v).mul_add(y1, y2)),
                x2 * (-1.0 + uv_v).mul_add(y3, uv_u.mul_add(-y1, y1)),
            ),
        );
        let px = (uv_v * x3).mul_add(
            x2.mul_add(y1, -(x1 * y2)),
            uv_u * x1 * x3.mul_add(y2, -(x2 * y3)),
        ) / denom;
        let py = (uv_v * y3).mul_add(
            x2.mul_add(y1, -(x1 * y2)),
            uv_u * y1 * x3.mul_add(y2, -(x2 * y3)),
        ) / denom;

        k0 + DVec2::new(px, py)
    }

    /// Builds an axis-aligned quad from two opposite corners.
    #[inline]
    #[must_use]
    pub fn make_rect(top_left: DVec2, bottom_right: DVec2) -> Self {
        Self {
            q0: DVec2::new(top_left.x, top_left.y),
            q1: DVec2::new(bottom_right.x, top_left.y),
            q2: DVec2::new(bottom_right.x, bottom_right.y),
            q3: DVec2::new(top_left.x, bottom_right.y),
        }
    }

    /// Calculates an inner quad from a rectangle specified in UV coordinates on the outer (i.e. `self`) quad.
    #[inline]
    #[must_use]
    pub fn inner(&self, top_left_uv: DVec2, bottom_right_uv: DVec2) -> Quad {
        let uv = Quad::make_rect(top_left_uv, bottom_right_uv);
        Quad {
            q0: self.uv_to_xy(uv.q0),
            q1: self.uv_to_xy(uv.q1),
            q2: self.uv_to_xy(uv.q2),
            q3: self.uv_to_xy(uv.q3),
        }
    }

    /// Calculates an outer quad, assuming this (inner) quad is a rectangle specified as
    /// UV coordinate corners `top_left_uv` and `bottom_right_uv` within the desired outer quad.
    #[inline]
    #[must_use]
    pub fn outer(&self, top_left_uv: DVec2, bottom_right_uv: DVec2) -> Quad {
        let denom = bottom_right_uv - top_left_uv;
        let lo = -top_left_uv / denom;
        let hi = DVec2::new(1.0 - top_left_uv.x, 1.0 - top_left_uv.y) / denom;
        let uv_quad = Quad::make_rect(lo, hi);
        Quad {
            q0: self.uv_to_xy(uv_quad.q0),
            q1: self.uv_to_xy(uv_quad.q1),
            q2: self.uv_to_xy(uv_quad.q2),
            q3: self.uv_to_xy(uv_quad.q3),
        }
    }
}

type SerdeQuad = (f64, f64, f64, f64, f64, f64, f64, f64);
impl From<SerdeQuad> for Quad {
    fn from(value: SerdeQuad) -> Self {
        Quad {
            q0: DVec2::new(value.0, value.1),
            q1: DVec2::new(value.2, value.3),
            q2: DVec2::new(value.4, value.5),
            q3: DVec2::new(value.6, value.7),
        }
    }
}

impl From<Quad> for SerdeQuad {
    fn from(value: Quad) -> Self {
        (
            value.q0.x, value.q0.y, value.q1.x, value.q1.y, value.q2.x, value.q2.y, value.q3.x,
            value.q3.y,
        )
    }
}

/// The default distance of the screen plane from the camera in script units.
const SCREEN_Z: f64 = 10000.0 / 32.0;
static_assertions::const_assert!((SCREEN_Z - 312.5).abs() < f64::EPSILON);

/// Calculates the effective screenZ after LayoutRes rescaling.
#[inline]
#[must_use]
pub fn rescale_screen_z(
    script_resolution: subtitle::Resolution,
    layout_resolution: subtitle::Resolution,
) -> f64 {
    SCREEN_Z * f64::from(script_resolution.y) / f64::from(layout_resolution.y)
}

/// Solves the 2x2 linear system
/// ```text
/// | a11 a12 | | x1 |   | b1 |
/// | a21 a22 | | x2 | = | b2 |
/// ```
/// returning `(x1, x2)`, where `target = (b1, b2)`,
/// `a1 = (a11, a21)`, and `a2 = (a12, a22)`.
#[inline]
#[must_use]
fn solve_2x2(a1: DVec2, a2: DVec2, target: DVec2) -> DVec2 {
    let matrix = DMat2::from_cols(a1, a2);
    match matrix.try_inverse() {
        Some(inv) => inv * target,
        None => DVec2::new(f64::NAN, f64::NAN),
    }
}

/// The `\org` mode used when reducing an inner quad to text tags.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrgMode {
    /// `\org` is placed at the quad's center.
    Center,
    /// `\org` is chosen so the quad unprojects to a rectangle
    /// (no shearing = no `\fax`).
    NoFax,
    /// `\org` is kept at whatever the caller already had.
    Keep(Position),
}

/// Calculates the perspective override tags that make a subtitle's base rectangle project onto the given quad.
///
/// Inputs are in **script** coordinates.
/// The caller must convert from screen coords, if necessary.
///
/// `bounding_box` is the bounding box of the sign before transformation,
/// to be calculated using text metrics or the like.
///
/// Returns a `Perspective` object that can be converted into ASS override tags later on.
#[must_use]
pub fn quad_to_tags(
    quad: &Quad,
    org_mode: OrgMode,
    alignment: Alignment,
    bounding_box: BoundingBox,
    screen_z: f64,
) -> Perspective {
    // Find a parallelogram projecting to the quad. Independent of translation.
    let diag = quad.q2 - quad.q0;
    let side2 = quad.q1 - quad.q2;
    let side3 = quad.q3 - quad.q2;
    let z_vector = solve_2x2(side2, side3, -diag);
    let (z1, z3) = (z_vector[0], z_vector[1]);

    let midpoint = quad.midpoint();

    let org = match org_mode {
        OrgMode::Center => midpoint,
        OrgMode::Keep(previous) => previous.into(),
        OrgMode::NoFax => calculate_no_fax_org(quad, z1, z3, screen_z),
    };

    // Normalize to the chosen `\org`.
    let q0 = quad.q0 - org;
    let q1 = quad.q1 - org;
    let q2 = quad.q2 - org;
    let q3 = quad.q3 - org;

    // Lift the quad into the reconstructed 3D parallelogram.
    let r0 = DVec3::new(q0.x, q0.y, screen_z);
    let r1 = z1 * DVec3::new(q1.x, q1.y, screen_z);
    let r2 = (z1 + z3 - 1.0) * DVec3::new(q2.x, q2.y, screen_z);
    let r3 = z3 * DVec3::new(q3.x, q3.y, screen_z);
    let mut parallelogram = [r0, r1, r2, r3];

    // Find the z coordinate of the point projecting to the origin.
    let top_side = r1 - r0;
    let left_side = r3 - r0;
    let orgla = solve_2x2(top_side.xy(), left_side.xy(), (-r0).xy());
    let (orgla0, orgla1) = (orgla[0], orgla[1]);
    let oz = (r0 + orgla0 * top_side + orgla1 * left_side).z;

    // Normalize so the origin has z=screen_z, and move the screen plane to z=0.
    let z_offset = DVec3::new(0.0, 0.0, screen_z);
    for corner in &mut parallelogram {
        *corner = *corner * screen_z / oz - z_offset;
    }

    // Find the normal vector of the parallelogram we want to rotate.
    let normal = (parallelogram[1] - parallelogram[0]).cross(parallelogram[3] - parallelogram[0]);

    // Find the X and Y rotation angles.
    let rot_y = normal.x.atan2(normal.z);
    let normal_rotated = rotate_y(normal, rot_y);
    let rot_x = normal_rotated.y.atan2(normal_rotated.z);

    // Rotate parallelogram into the screen plane (z = 0).
    for corner in &mut parallelogram {
        *corner = rotate_x(rotate_y(*corner, rot_y), rot_x);
    }

    // Find the Z rotation angle.
    let top_edge = parallelogram[1] - parallelogram[0];
    let rot_z = top_edge.y.atan2(top_edge.x);

    // Rotate parallelogram again to make the top side horizontal.
    for corner in &mut parallelogram {
        *corner = rotate_z(*corner, -rot_z);
    }

    // Now we have a horizontal parallelogram in the plane.
    // Next, find shear and dimensions.
    let left_edge = parallelogram[3] - parallelogram[0];
    let raw_fax = left_edge.x / left_edge.y;

    let width = top_edge.length();
    let height = left_edge.y.abs();
    let (bbox_min, bbox_max): (DVec2, DVec2) = (
        bounding_box.top_left.into(),
        bounding_box.bottom_right.into(),
    );
    let scale_x = width / (bbox_max.x - bbox_min.x).max(1.0);
    let scale_y = height / (bbox_max.y - bbox_min.y).max(1.0);
    let scale = DVec2::new(scale_x, scale_y);

    let shift_v = alignment.vertical.shift_factor();
    let shift_h = alignment.horizontal.shift_factor();

    let top_left_corner_xy = parallelogram[0].xy();
    let pos =
        org + top_left_corner_xy - bbox_min / scale + DVec2::new(width * shift_h, height * shift_v);

    Perspective {
        pos,
        org,
        scale,
        rot_x,
        rot_y,
        rot_z,
        raw_fax,
    }
}

#[expect(clippy::min_ident_chars, reason = "mathematical identifiers")]
fn calculate_no_fax_org(quad: &Quad, z1: f64, z3: f64, screen_z: f64) -> DVec2 {
    let v1 = quad.q1 - quad.q0;
    let v3 = quad.q3 - quad.q0;

    // Look for a translation after which the quad unprojects to a
    // rectangle. The set of valid translations t is cut out by
    // a*(x^2 + y^2) - b.x - b.y + c = 0 with these coefficients.
    let a = (1.0 - z1) * (1.0 - z3);
    let b = z1 * v1 + z3 * v3 - z1 * z3 * (v1 + v3);
    let c = (z1 * z3).mul_add(v1.dot(v3), (z1 - 1.0) * (z3 - 1.0) * screen_z * screen_z);

    // Default t puts \org at the center of the quad; find the valid t
    // closest to it.
    let mut t = quad.q0 - quad.midpoint();

    if a == 0.0 {
        // Degenerate: the equation cuts out a line (or is trivial).
        if b.length_squared() != 0.0 {
            t += b * ((c - t.dot(b)) / b.length_squared());
        }
        quad.q0 - t
    } else {
        // The equation cuts out a circle. Complete the square.
        let circle_center = b / (2.0 * a);
        let sqradius = (b.length_squared() / (4.0 * a) - c) / a;
        if sqradius <= 0.0 {
            // Very rare: \org is the circle center directly.
            circle_center
        } else {
            let radius = sqradius.sqrt();
            let center2t = t - circle_center;
            let len = center2t.length();
            t = if len == 0.0 {
                circle_center + DVec2::new(radius, 0.0)
            } else {
                circle_center + center2t / len * radius
            };
            quad.q0 - t
        }
    }
}

const RAD2DEG: f64 = 180.0 / std::f64::consts::PI;

#[derive(Debug, Clone)]
pub struct Perspective {
    pos: DVec2,
    org: DVec2,
    scale: DVec2,
    rot_x: f64,
    rot_y: f64,
    rot_z: f64,
    raw_fax: f64,
}

impl Perspective {
    /// Converts this perspective data into ASS override tags.
    /// Takes as input previous values (from previous override tags, or style information).
    /// The given `global` tags are modified in place. New `local` tags are returned
    /// that can be used in `override_from`/`clear_from` as desired.
    pub fn apply(
        &self,
        global: &mut Global,
        old_font_scale: DVec2,
        old_border: DVec2,
        old_shadow: DVec2,
    ) -> Option<Local> {
        let angle_x = self.rot_x * RAD2DEG;
        let angle_y = -self.rot_y * RAD2DEG;
        let angle_z = -self.rot_z * RAD2DEG;

        let new_font_scale = self.scale;
        let fax = self.raw_fax * self.scale.y / self.scale.x;
        let fay = 0.0;

        // Border and shadow scale with the change in fsc (component-wise).
        let border_shadow_ratio = new_font_scale / old_font_scale;
        let new_border = old_border * border_shadow_ratio;
        let new_shadow = old_shadow * border_shadow_ratio;

        let is_finite = angle_x.is_finite()
            && angle_y.is_finite()
            && angle_z.is_finite()
            && fax.is_finite()
            && vector_is_finite(self.org)
            && vector_is_finite(self.pos)
            && vector_is_finite(new_font_scale)
            && vector_is_finite(new_border)
            && vector_is_finite(new_shadow);

        is_finite.then(|| {
            global.position = Some(PositionOrMove::Position(self.pos.into()));
            global.origin = Some(self.org.into());

            Local {
                text_rotation: Maybe3D {
                    x: Resettable::Override(angle_x),
                    y: Resettable::Override(angle_y),
                    z: Resettable::Override(angle_z),
                },
                text_shear: DVec2::new(fax, fay).into(),
                font_scale: new_font_scale.into(),
                border: new_border.into(),
                shadow: new_shadow.into(),
                ..Local::empty()
            }
        })
    }
}

fn vector_is_finite(vector: DVec2) -> bool {
    vector.x.is_finite() && vector.y.is_finite()
}

/// The inverse of [`quad_to_tags`]: given the perspective tags,
/// project the sign's base rectangle to the four quad corners,
/// as specified by the rotations, scalings, etc. in the `Perspective` object.
#[must_use]
pub fn tags_to_quad(
    perspective: &Perspective,
    alignment: Alignment,
    bounding_box: BoundingBox,
    screen_z: f64,
) -> Quad {
    let (bbox_min, bbox_max): (DVec2, DVec2) = (
        bounding_box.top_left.into(),
        bounding_box.bottom_right.into(),
    );
    let text_width = (bbox_max.x - bbox_min.x).max(1.0);
    let text_height = (bbox_max.y - bbox_min.y).max(1.0);

    let shift_x = (-text_width) * alignment.horizontal.shift_factor();
    let shift_y = (-text_height) * alignment.vertical.shift_factor();

    let fax = perspective.raw_fax * perspective.scale.y / perspective.scale.x;

    let rect = Quad::make_rect(bbox_min, bbox_max);
    let mut out = [DVec2::ZERO; 4];

    for (i, &corner) in [rect.q0, rect.q1, rect.q2, rect.q3].iter().enumerate() {
        let mut point = corner;

        // Apply \fax and \fay
        point = DVec2::new(point.y.mul_add(fax, point.x), point.y);

        // Translate to alignment point
        point += DVec2::new(shift_x, shift_y);

        // Apply scaling
        point *= perspective.scale;

        // Translate relative to origin
        point += perspective.pos - perspective.org;

        // Rotate ZXY
        let mut point_3d = DVec3::new(point.x, point.y, 0.0);
        point_3d = rotate_z(point_3d, perspective.rot_z);
        point_3d = rotate_x(point_3d, -perspective.rot_x);
        point_3d = rotate_y(point_3d, -perspective.rot_y);

        // Project back onto screen
        point_3d *= screen_z / (point_3d.z + screen_z);

        // Move to origin
        out[i] = point_3d.xy() + perspective.org;
    }

    Quad {
        q0: out[0],
        q1: out[1],
        q2: out[2],
        q3: out[3],
    }
}

// Rotation helpers that exactly match Aegisub's handedness convention

#[inline]
fn rotate_x(vector: DVec3, theta: f64) -> DVec3 {
    // y' = cos*y - sin*z ; z' = sin*y + cos*z  (standard right-handed X)
    DMat3::from_rotation_x(theta) * vector
}

#[inline]
fn rotate_y(vector: DVec3, theta: f64) -> DVec3 {
    // Aegisub: x' = cos*x - sin*z ; z' = sin*x + cos*z.
    // The standard right-handed Y-rotation is x' = cos*x + sin*z, so we negate.
    DMat3::from_rotation_y(-theta) * vector
}

#[inline]
fn rotate_z(vector: DVec3, theta: f64) -> DVec3 {
    // x' = cos*x - sin*y ; y' = sin*x + cos*y  (standard right-handed Z)
    DMat3::from_rotation_z(theta) * vector
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_float_eq::assert_float_absolute_eq;
    use assert_matches2::assert_matches;

    const DEG2RAD: f64 = std::f64::consts::PI / 180.0;

    fn quad_approx_equal(quad1: &Quad, quad2: &Quad, epsilon: f64) -> bool {
        let result = (quad1.q0.x - quad2.q0.x).abs() < epsilon
            && (quad1.q0.y - quad2.q0.y).abs() < epsilon
            && (quad1.q1.x - quad2.q1.x).abs() < epsilon
            && (quad1.q1.y - quad2.q1.y).abs() < epsilon
            && (quad1.q2.x - quad2.q2.x).abs() < epsilon
            && (quad1.q2.y - quad2.q2.y).abs() < epsilon
            && (quad1.q3.x - quad2.q3.x).abs() < epsilon
            && (quad1.q3.y - quad2.q3.y).abs() < epsilon;

        if !result {
            println!("Quad 1: {quad1:?}");
            println!("Quad 2: {quad2:?}");
        }

        result
    }

    fn sample_irregular_quad() -> Quad {
        Quad {
            q0: DVec2::new(100.0, 120.0),
            q1: DVec2::new(540.0, 90.0),
            q2: DVec2::new(600.0, 420.0),
            q3: DVec2::new(60.0, 400.0),
        }
    }

    #[test]
    fn uv_corners_are_canonical() {
        let quad = sample_irregular_quad();

        let uv0 = quad.xy_to_uv(quad.q0);
        assert_float_absolute_eq!(uv0.x, 0.0, 1e-9);
        assert_float_absolute_eq!(uv0.y, 0.0, 1e-9);

        let uv1 = quad.xy_to_uv(quad.q1);
        assert_float_absolute_eq!(uv1.x, 1.0, 1e-9);
        assert_float_absolute_eq!(uv1.y, 0.0, 1e-9);

        let uv2 = quad.xy_to_uv(quad.q2);
        assert_float_absolute_eq!(uv2.x, 1.0, 1e-9);
        assert_float_absolute_eq!(uv2.y, 1.0, 1e-9);

        let uv3 = quad.xy_to_uv(quad.q3);
        assert_float_absolute_eq!(uv3.x, 0.0, 1e-9);
        assert_float_absolute_eq!(uv3.y, 1.0, 1e-9);
    }

    #[test]
    fn xy_uv_roundtrip() {
        let quad = sample_irregular_quad();
        let point = DVec2::new(333.0, 250.0);

        let round_trip_point = quad.uv_to_xy(quad.xy_to_uv(point));
        assert_float_absolute_eq!(point.x, round_trip_point.x, 1e-9);
        assert_float_absolute_eq!(point.y, round_trip_point.y, 1e-9);
    }

    #[test]
    fn convexity() {
        assert!(sample_irregular_quad().is_convex());

        // Self-intersecting “bowtie” ordering is not convex.
        let bowtie = Quad {
            q0: DVec2::new(0.0, 0.0),
            q1: DVec2::new(100.0, 0.0),
            q2: DVec2::new(0.0, 100.0),
            q3: DVec2::new(100.0, 100.0),
        };
        assert!(!bowtie.is_convex());
    }

    #[test]
    fn inner_outer_roundtrip() {
        let outer = sample_irregular_quad();
        let c1 = DVec2::new(0.2, 0.15);
        let c2 = DVec2::new(0.8, 0.7);

        let inner = outer.inner(c1, c2);
        let outer2 = inner.outer(c1, c2);

        assert!(quad_approx_equal(&outer, &outer2, 1e-9));
    }

    fn persp_quad_1() -> (Perspective, Alignment, BoundingBox, f64) {
        let res = subtitle::Resolution { x: 1920, y: 1080 };
        let screen_z = rescale_screen_z(res, res);

        let tags = Perspective {
            org: DVec2::new(960.0, 540.0),
            pos: DVec2::new(900.0, 500.0),
            rot_x: 22.0 * DEG2RAD,
            rot_y: 15.0 * DEG2RAD,
            rot_z: -7.0 * DEG2RAD,
            raw_fax: 0.0,
            scale: DVec2::new(1.2, 0.95),
        };

        let bounding_box = BoundingBox {
            top_left: Position::new(0.0, 0.0),
            bottom_right: Position::new(300.0, 80.0),
        };
        let align = Alignment::try_from_an(5).unwrap();

        (tags, align, bounding_box, screen_z)
    }

    #[test]
    fn persp_quad_roundtrip_keep() {
        let (tags, align, bounding_box, screen_z) = persp_quad_1();

        let quad = tags_to_quad(&tags, align, bounding_box, screen_z);
        let rec = quad_to_tags(
            &quad,
            OrgMode::Keep(tags.org.into()),
            align,
            bounding_box,
            screen_z,
        );
        let quad2 = tags_to_quad(&rec, align, bounding_box, screen_z);

        assert_float_absolute_eq!(tags.rot_x, rec.rot_x, 1e-6);
        assert_float_absolute_eq!(tags.rot_y, rec.rot_y, 1e-6);
        assert_float_absolute_eq!(tags.rot_y, rec.rot_y, 1e-6);
        assert_float_absolute_eq!(tags.raw_fax, rec.raw_fax, 1e-6);
        assert_float_absolute_eq!(tags.org.x, rec.org.x, 1e-6);
        assert_float_absolute_eq!(tags.org.y, rec.org.y, 1e-6);
        assert_float_absolute_eq!(tags.pos.x, rec.pos.x, 1e-6);
        assert_float_absolute_eq!(tags.pos.y, rec.pos.y, 1e-6);
        assert_float_absolute_eq!(tags.scale.x, rec.scale.x, 1e-6);
        assert_float_absolute_eq!(tags.scale.y, rec.scale.y, 1e-6);

        assert!(quad_approx_equal(&quad, &quad2, 1e-6));
    }

    #[test]
    fn persp_quad_roundtrip_center() {
        let (tags, align, bounding_box, screen_z) = persp_quad_1();

        let quad = tags_to_quad(&tags, align, bounding_box, screen_z);
        assert!(quad.is_convex());
        let rec = quad_to_tags(&quad, OrgMode::Center, align, bounding_box, screen_z);
        let quad2 = tags_to_quad(&rec, align, bounding_box, screen_z);

        assert!(quad_approx_equal(&quad, &quad2, 1e-6));
    }

    #[test]
    fn persp_nofax_zeroes_shear() {
        let res = subtitle::Resolution { x: 1920, y: 1080 };
        let screen_z = rescale_screen_z(res, res);

        let tags = Perspective {
            org: DVec2::new(960.0, 540.0),
            pos: DVec2::new(850.0, 480.0),
            rot_x: 18.0 * DEG2RAD,
            rot_y: 22.0 * DEG2RAD,
            rot_z: -11.0 * DEG2RAD,
            raw_fax: 0.25,
            scale: DVec2::new(1.1, 0.9),
        };

        let bounding_box = BoundingBox {
            top_left: Position::new(0.0, 0.0),
            bottom_right: Position::new(280.0, 70.0),
        };
        let align = Alignment::try_from_an(5).unwrap();
        let quad = tags_to_quad(&tags, align, bounding_box, screen_z);
        assert!(quad.is_convex());
        let rec = quad_to_tags(&quad, OrgMode::NoFax, align, bounding_box, screen_z);
        assert!(rec.raw_fax.abs() < 1e-6);

        let quad2 = tags_to_quad(&rec, align, bounding_box, screen_z);
        assert!(quad_approx_equal(&quad, &quad2, 1e-6));
    }

    #[test]
    fn known_values() {
        // Values from https://mz.sb/persp
        let quad = Quad {
            q0: DVec2::new(220.0, 130.0),
            q1: DVec2::new(620.0, 160.0),
            q2: DVec2::new(560.0, 370.0),
            q3: DVec2::new(200.0, 340.0),
        };

        let (alpha, beta) = quad.diagonal_intersection();
        assert_float_absolute_eq!(alpha, 0.522, 0.001);
        assert_float_absolute_eq!(-beta, 0.470, 0.001);

        let midpoint = quad.midpoint();
        assert_float_absolute_eq!(midpoint.x, 397.6, 0.1);
        assert_float_absolute_eq!(midpoint.y, 255.3, 0.1);

        let res = subtitle::Resolution { x: 800, y: 480 };
        let screen_z = rescale_screen_z(res, res);

        let bounding_box = BoundingBox {
            top_left: Position::new(0.0, 0.0),
            bottom_right: Position::new(200.0, 60.0),
        };
        let align = Alignment::try_from_an(7).unwrap();

        let perspective_center =
            quad_to_tags(&quad, OrgMode::Center, align, bounding_box, screen_z);
        let perspective_nofax = quad_to_tags(&quad, OrgMode::NoFax, align, bounding_box, screen_z);
        let perspective_keep = quad_to_tags(
            &quad,
            OrgMode::Keep(Position::new(400.0, 240.0)),
            align,
            bounding_box,
            screen_z,
        );

        assert_float_absolute_eq!(perspective_center.org.x, 397.6, 0.1);
        assert_float_absolute_eq!(perspective_center.org.y, 255.3, 0.1);
        assert_float_absolute_eq!(perspective_nofax.org.x, 173.6, 0.1);
        assert_float_absolute_eq!(perspective_nofax.org.y, 255.2, 0.1);
        assert_float_absolute_eq!(perspective_keep.org.x, 400.0, 0.1);
        assert_float_absolute_eq!(perspective_keep.org.y, 240.0, 0.1);

        assert_float_absolute_eq!(perspective_center.rot_y, 1.383 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_center.rot_x, -8.538 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_center.rot_z, 4.589 * DEG2RAD, 0.001);

        assert_float_absolute_eq!(perspective_nofax.rot_y, 1.407 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_nofax.rot_x, -8.685 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_nofax.rot_z, 4.631 * DEG2RAD, 0.001);

        assert_float_absolute_eq!(perspective_keep.rot_y, 1.372 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_keep.rot_x, -8.474 * DEG2RAD, 0.001);
        assert_float_absolute_eq!(perspective_keep.rot_z, 4.554 * DEG2RAD, 0.001);

        assert_float_absolute_eq!(perspective_center.scale.x, 1.901, 0.001);
        assert_float_absolute_eq!(perspective_center.scale.y, 3.572, 0.001);
        let fax =
            perspective_center.raw_fax * perspective_center.scale.y / perspective_center.scale.x;
        assert_float_absolute_eq!(fax, -0.2043, 0.0001);

        assert_float_absolute_eq!(perspective_nofax.scale.x, 1.852, 0.001);
        assert_float_absolute_eq!(perspective_nofax.scale.y, 3.482, 0.001);
        assert_float_absolute_eq!(perspective_nofax.raw_fax, 0.0, 0.0001);

        assert_float_absolute_eq!(perspective_keep.scale.x, 1.915, 0.001);
        assert_float_absolute_eq!(perspective_keep.scale.y, 3.626, 0.001);
        let fax = perspective_keep.raw_fax * perspective_keep.scale.y / perspective_keep.scale.x;
        assert_float_absolute_eq!(fax, -0.2065, 0.0001);

        assert_float_absolute_eq!(perspective_center.pos.x, 219.1, 0.1);
        assert_float_absolute_eq!(perspective_center.pos.y, 148.2, 0.1);
        assert_float_absolute_eq!(perspective_nofax.pos.x, 207.9, 0.1);
        assert_float_absolute_eq!(perspective_nofax.pos.y, 133.1, 0.1);
        assert_float_absolute_eq!(perspective_keep.pos.x, 219.1, 0.1);
        assert_float_absolute_eq!(perspective_keep.pos.y, 147.0, 0.1);
    }

    #[test]
    fn apply() {
        let (tags, _, _, _) = persp_quad_1();

        let mut global = Global::empty();

        let scale = DVec2::new(0.9, 1.19);
        let shadow = DVec2::new(2.0, 3.0);
        let border = DVec2::new(4.0, 5.0);

        let local = tags.apply(&mut global, scale, border, shadow);
        assert_matches!(local, Some(local));

        assert_matches!(global.origin, Some(org));
        assert_float_absolute_eq!(org.x, tags.org.x, 1e-9);
        assert_float_absolute_eq!(org.y, tags.org.y, 1e-9);

        assert_matches!(global.position, Some(PositionOrMove::Position(pos)));
        assert_float_absolute_eq!(pos.x, tags.pos.x, 1e-9);
        assert_float_absolute_eq!(pos.y, tags.pos.y, 1e-9);

        assert_matches!(local.font_scale.x, Resettable::Override(fscx));
        assert_matches!(local.shadow.x, Resettable::Override(xshad));
        assert_float_absolute_eq!(xshad, shadow.x * fscx / scale.x, 1e-9);

        assert_matches!(local.text_rotation.y, Resettable::Override(fry));
        assert_float_absolute_eq!(fry, -tags.rot_y * RAD2DEG, 1e-9);

        assert_matches!(local.text_shear.y, Resettable::Override(0.0));
    }
}
