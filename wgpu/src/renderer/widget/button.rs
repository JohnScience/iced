use crate::{button::StyleSheet, defaults, Defaults, Primitive, Renderer};
use iced_native::{
    Background, Color, Element, Layout, MouseCursor, Point, Rectangle,
};

impl iced_native::button::Renderer for Renderer {
    type Style = Box<dyn StyleSheet>;

    fn draw<Message>(
        &mut self,
        defaults: &Defaults,
        bounds: Rectangle,
        cursor_position: Point,
        is_disabled: bool,
        is_pressed: bool,
        style: &Box<dyn StyleSheet>,
        content: &Element<'_, Message, Self>,
        content_layout: Layout<'_>,
    ) -> Self::Output {
        let is_mouse_over = bounds.contains(cursor_position);

        // TODO: Render proper shadows
        let styling = if is_disabled {
            style.disabled()
        } else if is_mouse_over {
            if is_pressed {
                style.pressed()
            } else {
                style.hovered()
            }
        } else {
            style.active()
        };

        let (content, _) = content.draw(
            self,
            &Defaults {
                text: defaults::Text {
                    color: styling.text_color,
                },
                ..*defaults
            },
            content_layout,
            cursor_position,
        );

        (
            match styling.background {
                None => content,
                Some(background) => Primitive::Group {
                    primitives: vec![
                        Primitive::Quad {
                            bounds: Rectangle {
                                x: bounds.x + styling.shadow_offset.x,
                                y: bounds.y + styling.shadow_offset.y,
                                ..bounds
                            },
                            background: Background::Color(
                                [0.0, 0.0, 0.0, 0.5].into(),
                            ),
                            border_radius: styling.border_radius,
                            border_width: 0,
                            border_color: Color::TRANSPARENT,
                        },
                        Primitive::Quad {
                            bounds,
                            background,
                            border_radius: styling.border_radius,
                            border_width: styling.border_width,
                            border_color: styling.border_color,
                        },
                        content,
                    ],
                },
            },
            if is_mouse_over {
                MouseCursor::Pointer
            } else {
                MouseCursor::OutOfBounds
            },
        )
    }
}
