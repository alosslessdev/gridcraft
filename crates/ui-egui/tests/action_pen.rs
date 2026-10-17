//! Draw ▸ Action Pen: a stroke written in the grid is recognized and lands as typed cell data.

use egui::{Event, Modifiers, PointerButton};
use gridcraft_engine::Session;
use gridcraft_engine::core::{CellRef, Value};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn harness(session: Session) -> egui_kittest::Harness<'static, SheetApp> {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx);
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    );
    h.run_steps(4);
    h
}

struct FixedRecognizer;

impl gridcraft_engine::handwriting::HandwritingRecognizer for FixedRecognizer {
    fn recognize(&self, _strokes: &[Vec<[f32; 2]>], _language: &str) -> Result<String, String> {
        Ok("37".into())
    }
}

#[test]
fn action_pen_writes_recognized_text_into_the_cell_under_the_stroke() {
    let mut s = Session::new();
    s.new_workbook();
    s.handwriting = Some(std::sync::Arc::new(FixedRecognizer));
    let mut h = harness(s);
    h.state_mut().run("draw.actionPen", json!({"on": true})).unwrap();

    let app = h.state();
    let sheet = app.session.active().unwrap().wb.active().unwrap();
    let geo = gridcraft_ui_egui::grid::Geo::new(sheet, app.grid.rect.unwrap(), app.view().scroll);
    let b2 = geo.cell_rect(sheet, CellRef::new(1, 1));
    let start = b2.center() - egui::vec2(12.0, 6.0);
    let mid = b2.center();
    let end = b2.center() + egui::vec2(12.0, 6.0);
    h.input_mut().events.extend([
        Event::PointerMoved(start),
        Event::PointerButton { pos: start, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE },
        Event::PointerMoved(mid),
    ]);
    h.run_steps(3);
    h.input_mut().events.push(Event::PointerMoved(end));
    h.run_steps(3);
    h.input_mut().events.push(Event::PointerButton { pos: end, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(3);

    let value = h.state().session.active().unwrap().wb.active().unwrap().value(CellRef::new(1, 1));
    assert_eq!(value, Value::Number(37.0), "the recognized number is entered in B2");
    // The Action Pen recognizes instead of leaving an ink stroke behind.
    assert_eq!(h.state().session.active().unwrap().wb.active().unwrap().shapes.len(), 0);
}
