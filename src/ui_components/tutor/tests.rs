use super::*;

fn enter(modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

struct Harness {
    ctx: egui::Context,
    input: String,
    cache: CommonMarkCache,
    busy: bool,
    size: egui::Vec2,
}

impl Harness {
    fn new(size: egui::Vec2) -> Self {
        let ctx = egui::Context::default();

        Self {
            ctx,
            input: String::new(),
            cache: Default::default(),
            busy: false,
            size,
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) -> (Option<Action>, egui::FullOutput) {
        let mut action = None;
        let messages = [Dialogue::new("assistant", "讲解内容。\n\n".repeat(100))];
        let mut output = self.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)),
                events,
                ..Default::default()
            },
            |ui| {
                action = TutorPanel {
                    id: panel_id("test", "card"),
                    input: &mut self.input,
                    messages: &messages,
                    reference: None,
                    busy: self.busy,
                    generating: self.busy,
                    partial: "正在解释…",
                }
                .show(ui, &mut self.cache);
            },
        );
        output.textures_delta.clear();
        (action, output)
    }
}

#[test]
fn long_pasted_draft_keeps_send_visible_and_clickable_after_resizing() {
    let mut h = Harness::new(egui::vec2(360., 600.));
    h.frame(vec![]);
    h.ctx
        .memory_mut(|m| m.request_focus(input_id(panel_id("test", "card"))));
    let draft = "import java.util.*;\npublic class Main { /* 长代码 */ }\n".repeat(150);
    h.frame(vec![egui::Event::Paste(draft.clone())]);
    assert_eq!(h.input, draft);
    for size in [
        egui::vec2(280., 330.),
        egui::vec2(390., 700.),
        egui::vec2(320., 440.),
    ] {
        h.size = size;
        let mut output = h.frame(vec![]).1;
        for _ in 0..2 {
            output = h.frame(vec![]).1;
        }
        let rect = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(t) if t.galley.text() == "发送追问" => {
                    let rect = egui::Rect::from_min_size(t.pos, t.galley.size());
                    assert!(
                        shape.clip_rect.contains_rect(rect),
                        "send clipped: {size:?}"
                    );
                    Some(rect)
                }
                _ => None,
            })
            .expect("send button");
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(rect));
        let pos = rect.center();
        for pressed in [true, false] {
            let (action, _) = h.frame(vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]);
            if !pressed {
                assert!(matches!(action, Some(Action::Ask(ref s)) if s == &draft));
            }
        }
    }
}

#[test]
fn shortcut_requires_input_focus_and_preserves_newlines_and_busy_drafts() {
    let mut h = Harness::new(egui::vec2(360., 500.));
    h.input = "第一行".into();
    h.frame(vec![]);
    assert!(h.frame(vec![enter(egui::Modifiers::CTRL)]).0.is_none());
    h.ctx
        .memory_mut(|m| m.request_focus(input_id(panel_id("test", "card"))));
    h.frame(vec![]);
    let (action, _) = h.frame(vec![enter(egui::Modifiers::NONE)]);
    assert!(action.is_none());
    assert!(h.input.contains('\n'));
    let before = h.input.clone();
    let (action, _) = h.frame(vec![enter(egui::Modifiers::CTRL)]);
    assert!(matches!(action, Some(Action::Ask(ref s)) if s == &before));
    assert_eq!(h.input, before, "sending must not insert a newline");
    h.busy = true;
    assert!(h.frame(vec![enter(egui::Modifiers::CTRL)]).0.is_none());
    assert_eq!(h.input, before);
    h.busy = false;
    h.input = " \n ".into();
    h.ctx
        .memory_mut(|m| m.request_focus(input_id(panel_id("test", "card"))));
    assert!(h.frame(vec![enter(egui::Modifiers::CTRL)]).0.is_none());
}
