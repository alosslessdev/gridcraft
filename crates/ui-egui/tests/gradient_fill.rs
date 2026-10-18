//! Format Cells › Fill: creating, cancelling and replacing a gradient through real controls.

use egui::vec2;
use egui_kittest::kittest::Queryable;
use gridcraft_engine::Session;
use gridcraft_engine::core::CellRef;
use gridcraft_engine::model::{Fill, PatternType};
use gridcraft_ui_egui::SheetApp;
use serde_json::json;

fn harness() -> egui_kittest::Harness<'static, SheetApp> {
    let mut session = Session::new();
    session.new_workbook();
    session.run("range.setValues", json!({"range": "A1", "values": [[1]]})).unwrap();
    session.run("selection.set", json!({"range": "A1"})).unwrap();
    let mut h = egui_kittest::Harness::builder().with_size(vec2(1200.0, 800.0)).build_ui_state(
        |ui, app: &mut SheetApp| {
            app.logic(&ui.ctx().clone());
            app.ui(ui);
        },
        SheetApp::new(session, Default::default()),
    );
    h.run_steps(6);
    h
}

fn fill(h: &egui_kittest::Harness<'static, SheetApp>, cell: &str) -> Fill {
    let doc = h.state().session.active().unwrap();
    let sheet = doc.wb.active().unwrap();
    doc.wb.styles.get(sheet.style_id(CellRef::parse(cell).unwrap())).fill.clone()
}

#[test]
fn gradient_can_be_created_cancelled_and_replaced_by_solid() {
    let mut h = harness();
    assert_eq!(fill(&h, "A1"), Fill::default());

    // Cancel keeps the original fill.
    h.state_mut().open_dialog("formatCells", json!({"tab": "Fill"}));
    h.run_steps(6);
    h.get_by_label("Gradient").click();
    h.run_steps(6);
    h.get_by_label(" Cancel ").click();
    h.run_steps(6);
    assert_eq!(fill(&h, "A1"), Fill::default());

    // OK applies a two-stop linear gradient.
    h.state_mut().open_dialog("formatCells", json!({"tab": "Fill"}));
    h.run_steps(6);
    h.get_by_label("Gradient").click();
    h.run_steps(6);
    h.get_by_label("   OK   ").click();
    h.run_steps(6);
    let applied = fill(&h, "A1");
    let gradient = applied.gradient.as_ref().expect("gradient fill");
    assert!(gradient.is_linear());
    assert_eq!(gradient.stops.len(), 2);

    h.state_mut().run("edit.undo", json!({})).unwrap();
    assert_eq!(fill(&h, "A1"), Fill::default());

    // Switching the same cell to Solid clears the gradient rather than leaving both active.
    h.state_mut().open_dialog("formatCells", json!({"tab": "Fill"}));
    h.run_steps(6);
    h.get_by_label("Gradient").click();
    h.run_steps(6);
    h.get_by_label("Solid").click();
    h.run_steps(6);
    h.get_by_label("   OK   ").click();
    h.run_steps(6);
    let solid = fill(&h, "A1");
    assert_eq!(solid.pattern, PatternType::Solid);
    assert!(solid.gradient.is_none());
}
