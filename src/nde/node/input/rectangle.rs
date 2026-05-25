use super::{Context, Node, Shell, SocketType, SocketValue};
use crate::model::reticule;
use crate::{message, nde, subtitle};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InputRectangle {
    pub value: nde::tags::Rectangle,
}

impl InputRectangle {
    fn reticule_update_internal(&self, reticules: &mut [reticule::Reticule]) {
        assert_eq!(
            reticules.len(),
            4,
            "the required number of reticules should be present"
        ); // Elide bounds checks

        reticules[0].position = nde::tags::Position {
            x: f64::from(self.value.x1),
            y: f64::from(self.value.y1),
        };
        reticules[1].position = nde::tags::Position {
            x: f64::from(self.value.x2),
            y: f64::from(self.value.y1),
        };
        reticules[2].position = nde::tags::Position {
            x: f64::from(self.value.x1),
            y: f64::from(self.value.y2),
        };
        reticules[3].position = nde::tags::Position {
            x: f64::from(self.value.x2),
            y: f64::from(self.value.y2),
        };
    }
}

#[typetag::serde]
impl Node for InputRectangle {
    fn name(&self) -> &'static str {
        "Input: Rectangle"
    }

    fn category(&self) -> super::Category {
        super::Category::Input
    }

    fn desired_inputs(&self) -> &[SocketType] {
        &[]
    }

    fn predicted_outputs(&self) -> &[SocketType] {
        &[SocketType::Rectangle]
    }

    fn run(
        &'_ self,
        _inputs: &[&SocketValue],
        _context: &Context,
    ) -> anyhow::Result<Vec<SocketValue<'_>>> {
        Ok(vec![SocketValue::Rectangle(self.value)])
    }

    fn content<'a>(
        &self,
        _filter_index: subtitle::ExtradataId,
        _self_index: nde::graph::NodeId,
    ) -> iced::Element<'a, message::Message> {
        let column = iced::widget::column![iced::widget::text(format!(
            "({:.1}, {:.1}; {:.1}, {:.1})",
            self.value.x1, self.value.y1, self.value.x2, self.value.y2,
        )),];

        column
            .spacing(4.0)
            .width(iced::Length::Fill)
            .align_x(iced::Alignment::Center)
            .into()
    }

    fn reticule_activate(&mut self) -> Vec<reticule::Reticule> {
        let mut reticule_list = vec![
            reticule::Reticule {
                shape: reticule::Shape::CornerTopLeft,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerTopRight,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerBottomLeft,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
            reticule::Reticule {
                shape: reticule::Shape::CornerBottomRight,
                position: nde::tags::Position::default(),
                radius: 15.0,
            },
        ];

        self.reticule_update_internal(&mut reticule_list);
        reticule_list
    }

    fn reticule_update(
        &mut self,
        reticules: &mut reticule::Reticules,
        index: reticule::Index,
        new_position: nde::tags::Position,
    ) -> anyhow::Result<nde::tags::Position> {
        let mut new_value = self.value;

        let (x_mut, y_mut) = match index.0 {
            0 => {
                // top left
                (&mut new_value.x1, &mut new_value.y1)
            }
            1 => {
                // top right
                (&mut new_value.x2, &mut new_value.y1)
            }
            2 => {
                // bottom left
                (&mut new_value.x1, &mut new_value.y2)
            }
            3 => {
                // bottom right
                (&mut new_value.x2, &mut new_value.y2)
            }
            _ => {
                anyhow::bail!("Reticule index out of range: {}", index.0);
            }
        };

        #[expect(
            clippy::cast_possible_truncation,
            reason = "extremely large values not expected in UI code"
        )]
        let old_x = std::mem::replace(x_mut, new_position.x as i32);

        #[expect(
            clippy::cast_possible_truncation,
            reason = "extremely large values not expected in UI code"
        )]
        let old_y = std::mem::replace(y_mut, new_position.y as i32);

        if new_value.is_positive() {
            self.value = new_value;
            self.reticule_update_internal(&mut reticules.list);
        }

        Ok(nde::tags::Position {
            x: f64::from(old_x),
            y: f64::from(old_y),
        })
    }

    fn content_size(&self) -> iced::Size {
        iced::Size::new(200.0, 125.0)
    }
}

inventory::submit! {
    Shell::new(
        &["Input", "Rectangle"],
        || Box::new(InputRectangle {
            value: nde::tags::Rectangle {
                x1: 100,
                y1: 100,
                x2: 200,
                y2: 200,
            }
        })
    )
}
