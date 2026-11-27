use egui::{
    Context, Event, Modifiers, PointerButton, Pos2, RawInput, TouchDeviceId, TouchId, TouchPhase,
};

type ContactId = (TouchDeviceId, TouchId);

#[derive(Clone, Copy)]
pub(super) struct Pinch {
    pub from: Pos2,
    pub to: Pos2,
    pub scale: f32,
}

#[derive(Default)]
pub(super) struct TouchInput {
    fingers: Vec<(ContactId, Pos2)>,
    pointer: Option<ContactId>,
    zooming: bool,
    pub pinch: Option<Pinch>,
    pub cancelled_pointer: bool,
}

impl TouchInput {
    // egui-winit emulates the mouse with the first contact. Keep that behavior for one
    // contact, then reserve the zoom pair and give the pointer to the next contact.
    pub fn prepare(&mut self, ctx: &Context, input: &mut RawInput) {
        self.cancelled_pointer = false;
        let mut before = self.geometry();
        let mut pointer_events = Vec::new();
        let mut has_touch = false;

        for event in &input.events {
            let Event::Touch {
                device_id,
                id,
                phase,
                pos,
                ..
            } = *event
            else {
                continue;
            };
            has_touch = true;
            let id = (device_id, id);
            let finger = self.fingers.iter().position(|(key, _)| *key == id);
            match phase {
                TouchPhase::Start => {
                    let controls_pointer = if self.fingers.len() < 2 {
                        self.fingers.push((id, pos));
                        before = self.geometry();
                        if self.fingers.len() == 2 && !self.zooming {
                            self.zooming = true;
                            self.pointer = None;
                            self.cancel(ctx, input.modifiers, &mut pointer_events);
                        }
                        !self.zooming
                    } else {
                        self.pointer.is_none()
                    };
                    if controls_pointer {
                        self.pointer = Some(id);
                        pointer_events.push(Event::PointerMoved(pos));
                        pointer_events.push(button(pos, true, input.modifiers));
                    }
                }
                TouchPhase::Move => {
                    if let Some(i) = finger {
                        self.fingers[i].1 = pos;
                    }
                    if self.pointer == Some(id) {
                        pointer_events.push(Event::PointerMoved(pos));
                    }
                }
                TouchPhase::End | TouchPhase::Cancel => {
                    if let Some(i) = finger {
                        self.fingers.remove(i);
                        before = None;
                    }
                    if self.pointer == Some(id) {
                        self.pointer = None;
                        if phase == TouchPhase::Cancel {
                            self.cancel(ctx, input.modifiers, &mut pointer_events);
                        } else {
                            pointer_events.push(Event::PointerMoved(pos));
                            pointer_events.push(button(pos, false, input.modifiers));
                            pointer_events.push(Event::PointerGone);
                        }
                    }
                    if self.fingers.is_empty() && self.pointer.is_none() {
                        self.zooming = false;
                    }
                }
            }
        }

        self.pinch = before
            .zip(self.geometry())
            .and_then(|((from, span), (to, new_span))| {
                let scale = new_span / span;
                (scale.is_finite() && scale > 0.0).then_some(Pinch { from, to, scale })
            });
        if has_touch {
            input.events.retain(|event| {
                !matches!(
                    event,
                    Event::PointerMoved(_) | Event::PointerButton { .. } | Event::PointerGone
                )
            });
            input.events.extend(pointer_events);
        }
    }

    fn geometry(&self) -> Option<(Pos2, f32)> {
        let [(_, a), (_, b)] = self.fingers.as_slice() else {
            return None;
        };
        Some((a.lerp(*b, 0.5), a.distance(*b)))
    }

    fn cancel(&mut self, ctx: &Context, modifiers: Modifiers, events: &mut Vec<Event>) {
        events.clear();
        if let Some(origin) = ctx.input(|i| {
            i.pointer
                .primary_down()
                .then(|| i.pointer.press_origin())
                .flatten()
        }) {
            // A release alone would register as a click. Move beyond the click tolerance,
            // then remove the pointer so egui reports neither movement nor a fling.
            let tolerance = ctx.options(|o| o.input_options.max_click_dist);
            let pos = origin - egui::Vec2::X * (tolerance + 1.0);
            events.push(Event::PointerMoved(pos));
            events.push(button(pos, false, modifiers));
            events.push(Event::PointerGone);
            self.cancelled_pointer = true;
        }
    }
}

fn button(pos: Pos2, pressed: bool, modifiers: Modifiers) -> Event {
    Event::PointerButton {
        pos,
        button: PointerButton::Primary,
        pressed,
        modifiers,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{pos2, CentralPanel, Id, Rect, Sense};
    use TouchPhase::{Cancel, End, Move, Start};

    #[derive(Default)]
    struct Frame {
        down: bool,
        clicked: bool,
        dragged: bool,
        position: Option<Pos2>,
    }

    fn frame(ctx: &Context, touch: &mut TouchInput, contacts: &[(u64, TouchPhase, f32)]) -> Frame {
        let mut raw = RawInput::default();
        for &(id, phase, x) in contacts {
            let pos = pos2(x, 80.0);
            raw.events.push(Event::Touch {
                device_id: TouchDeviceId(0),
                id: TouchId(id),
                phase,
                pos,
                force: None,
            });
            // Include the emulated pointer on the first finger, as egui-winit does.
            if id == 1 {
                raw.events.push(Event::PointerMoved(pos));
            }
        }
        touch.prepare(ctx, &mut raw);
        let mut result = Frame::default();
        let _ = ctx.run(raw, |ctx| {
            CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_max(pos2(20.0, 30.0), pos2(280.0, 130.0));
                let region = ui.interact(rect, Id::new("region"), Sense::click_and_drag());
                let (down, position) =
                    ctx.input(|i| (i.pointer.primary_down(), i.pointer.hover_pos()));
                result = Frame {
                    down,
                    clicked: region.clicked(),
                    dragged: region.dragged(),
                    position,
                };
            });
        });
        result
    }

    #[test]
    fn pinch_cancels_first_drag_and_third_contact_moves_region() {
        let ctx = Context::default();
        let mut touch = TouchInput::default();
        frame(&ctx, &mut touch, &[]);
        assert!(frame(&ctx, &mut touch, &[(1, Start, 80.0)]).down);
        assert!(frame(&ctx, &mut touch, &[(1, Move, 90.0)]).dragged);

        let second = frame(&ctx, &mut touch, &[(1, Move, 70.0), (2, Start, 210.0)]);
        assert!(touch.cancelled_pointer);
        assert!(!second.down && !second.clicked && !second.dragged);
        assert!(frame(&ctx, &mut touch, &[(3, Start, 160.0)]).down);
        let third = frame(
            &ctx,
            &mut touch,
            &[(1, Move, 50.0), (2, Move, 240.0), (3, Move, 180.0)],
        );
        assert!(third.dragged);
        assert_eq!(third.position, Some(pos2(180.0, 80.0)));
        let pinch = touch.pinch.unwrap();
        assert_eq!(pinch.from, pos2(140.0, 80.0));
        assert_eq!(pinch.to, pos2(145.0, 80.0));
        assert_eq!(pinch.scale, 190.0 / 140.0);

        assert!(frame(&ctx, &mut touch, &[(1, End, 50.0), (3, Move, 200.0)]).dragged);
        let remaining = frame(&ctx, &mut touch, &[(3, End, 200.0), (2, Move, 230.0)]);
        assert!(!remaining.down && !remaining.clicked && !remaining.dragged);
        assert!(remaining.position.is_none());
        frame(&ctx, &mut touch, &[(2, End, 230.0)]);
        assert!(frame(&ctx, &mut touch, &[(1, Start, 80.0)]).down);
        assert!(frame(&ctx, &mut touch, &[(1, End, 80.0)]).clicked);
    }

    #[test]
    fn pinch_started_in_one_frame_never_clicks_and_clears_on_cancel() {
        let ctx = Context::default();
        let mut touch = TouchInput::default();
        frame(&ctx, &mut touch, &[]);
        let both = frame(
            &ctx,
            &mut touch,
            &[(1, Start, 60.0), (2, Start, 120.0), (2, Move, 150.0)],
        );
        assert!(!both.down && !both.clicked && !both.dragged);
        assert_eq!(touch.pinch.unwrap().scale, 1.5);

        assert!(frame(&ctx, &mut touch, &[(3, Start, 160.0)]).down);
        let cancelled = frame(
            &ctx,
            &mut touch,
            &[(1, Cancel, 60.0), (2, Cancel, 150.0), (3, Cancel, 160.0)],
        );
        assert!(touch.pinch.is_none());
        assert!(!cancelled.down && !cancelled.clicked && !cancelled.dragged);
        assert!(frame(&ctx, &mut touch, &[(2, Start, 100.0)]).down);
    }

    #[test]
    fn canvas_keeps_dragged_region_under_third_contact_during_zoom() {
        use crate::app::App;
        use eframe::App as _;
        use egui::vec2;
        use xmath_core::Worksheet;

        let ctx = Context::default();
        let cc = eframe::CreationContext::_new_kittest(ctx.clone());
        let mut app = App::new(&cc, [], false);
        app.palettes.clear();
        app.doc.ws = Worksheet::from_lines("value := 123\nother := 456");
        app.doc.ws.regions[0].x = 240.0;
        app.doc.ws.regions[0].y = 200.0;
        app.doc.ws.regions[1].x = 900.0;
        app.doc.ws.regions[1].y = 700.0;

        let tick = |app: &mut App, touches: &[(u64, TouchPhase, Pos2)]| {
            let mut raw = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 650.0))),
                events: touches
                    .iter()
                    .map(|&(id, phase, pos)| Event::Touch {
                        device_id: TouchDeviceId(0),
                        id: TouchId(id),
                        phase,
                        pos,
                        force: None,
                    })
                    .collect(),
                ..Default::default()
            };
            app.raw_input_hook(&ctx, &mut raw);
            ctx.run(raw, |ctx| {
                app.update(ctx, &mut eframe::Frame::_new_kittest())
            })
        };
        tick(&mut app, &[]);
        let region_pos = |app: &App| pos2(app.doc.ws.regions[0].x, app.doc.ws.regions[0].y);
        let origin = |app: &App| app.doc.view_min - app.doc.scroll_off;
        let over_region = |app: &App| origin(app) + region_pos(app).to_vec2() + vec2(9.0, 8.0);
        let third = over_region(&app);
        let swipe = third + vec2(30.0, 0.0);
        tick(&mut app, &[(1, Start, third)]);
        tick(&mut app, &[(1, Move, swipe)]);
        tick(&mut app, &[(1, End, swipe)]);
        assert_eq!(region_pos(&app), pos2(240.0, 200.0));
        assert!(app.doc.selection.is_empty());

        let third = over_region(&app);
        tick(&mut app, &[(1, Start, third)]);
        let selected = tick(&mut app, &[(1, End, third)]);
        assert_eq!(app.doc.selection, vec![app.doc.ws.regions[0].id]);
        assert!(app.doc.editing.is_none());

        let rendered_value = |frame: egui::FullOutput| {
            frame
                .shapes
                .into_iter()
                .find_map(|shape| {
                    let egui::Shape::Text(text) = shape.shape else {
                        return None;
                    };
                    let galley = &text.galley;
                    (galley.job.text == "value").then(|| {
                        (
                            galley.job.sections[0].format.font_id.size,
                            galley.rect.width(),
                        )
                    })
                })
                .expect("value is visible on the worksheet")
        };
        let (font_size, width) = rendered_value(selected);

        let third = over_region(&app);
        let second = third + vec2(120.0, 20.0);
        tick(&mut app, &[(1, Start, third), (2, Start, second)]);
        assert_eq!(region_pos(&app), pos2(240.0, 200.0));
        tick(&mut app, &[(3, Start, third)]);

        let moved = third + vec2(22.0, 18.0);
        let zoomed = tick(
            &mut app,
            &[
                (1, Move, third - vec2(30.0, 10.0)),
                (2, Move, second + vec2(35.0, 20.0)),
                (3, Move, moved),
            ],
        );
        assert!(app.doc.zoom > 1.0);
        let (zoomed_font_size, zoomed_width) = rendered_value(zoomed);
        assert_eq!(zoomed_font_size, font_size);
        assert!((zoomed_width - width * app.doc.zoom).abs() < 0.1);
        let grabbed = (moved - origin(&app)) / app.doc.zoom - region_pos(&app).to_vec2();
        assert!((grabbed - vec2(9.0, 8.0)).length() < 0.1);
    }
}
