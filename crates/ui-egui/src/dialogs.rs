//! Dialogs. Simple dialogs are forms (fields → a command with params); Format Cells, Insert
//! Function, Find & Replace, Sort, Name Manager and the command palette are custom.
//!
//! Every dialog is also reachable programmatically: `ui.dialog {name}` opens one,
//! `ui.dialog.set {field, value}` edits a field and `ui.dialog.confirm` presses OK.

use egui::{Color32, Key, Rect, Sense, Stroke, StrokeKind, pos2, vec2};

use crate::icons::{self, Icon};
use serde_json::{Map, Value as Json, json};

use crate::SheetApp;
use crate::theme;

#[derive(Clone, Debug, PartialEq)]
pub enum Field {
    Text { key: &'static str, label: &'static str },
    Password { key: &'static str, label: &'static str },
    Number { key: &'static str, label: &'static str },
    Choice { key: &'static str, label: &'static str, options: Vec<(&'static str, &'static str)> },
    Check { key: &'static str, label: &'static str },
    Note(String),
}

#[derive(Clone, Debug)]
pub struct Dialog {
    pub name: String,
    pub title: String,
    pub fields: Vec<Field>,
    pub values: Map<String, Json>,
    /// Command to run on OK (params = values, transformed by `build`).
    pub command: Option<&'static str>,
    pub tab: String,
    pub search: String,
    pub list_index: usize,
    pub result: Option<Json>,
    /// "Show password" is ticked: password fields show their text.
    pub show_password: bool,
    picture_inbox: Option<crate::Inbox>,
    picture_target: Option<(u64, usize)>,
    /// The dialog this one was opened from (Name Manager › New…): it comes back when this closes.
    back: Option<Box<Dialog>>,
}

impl Dialog {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Sets a form value. In Zoom, a Magnification choice is also the percentage `view.zoom` stores.
    pub(crate) fn set_value(&mut self, key: &str, value: Json) {
        if self.name == "zoom"
            && key == "preset"
            && let Some(p) = value.as_str().and_then(|s| s.parse::<f64>().ok())
        {
            self.values.insert("percent".into(), json!(p));
        }
        self.values.insert(key.into(), value);
    }

    fn form(name: &str, title: &str, command: &'static str, fields: Vec<Field>, defaults: Json) -> Dialog {
        Dialog {
            name: name.into(),
            title: title.into(),
            fields,
            values: defaults.as_object().cloned().unwrap_or_default(),
            command: Some(command),
            tab: String::new(),
            search: String::new(),
            list_index: 0,
            result: None,
            show_password: false,
            picture_inbox: None,
            picture_target: None,
            back: None,
        }
    }

    fn custom(name: &str, title: &str, defaults: Json) -> Dialog {
        Dialog {
            name: name.into(),
            title: title.into(),
            fields: vec![],
            values: defaults.as_object().cloned().unwrap_or_default(),
            command: None,
            tab: String::new(),
            search: String::new(),
            list_index: 0,
            result: None,
            show_password: false,
            picture_inbox: None,
            picture_target: None,
            back: None,
        }
    }

    pub fn open(app: &mut SheetApp, name: &str, params: Json) -> Option<Dialog> {
        let sel = app.session.active().map(|d| d.selection.current().a1()).unwrap_or_default();
        let active = app.session.active().map(|d| d.selection.active.a1()).unwrap_or_default();
        let p = |k: &str| params.get(k).cloned().unwrap_or(Json::Null);
        use Field::*;
        let d = match name {
            "insertCells" => Dialog::form(
                "insertCells",
                "Insert",
                "home.insertCells",
                vec![Choice {
                    key: "shift",
                    label: "Insert",
                    options: vec![("right", "Shift cells right"), ("down", "Shift cells down"), ("row", "Entire row"), ("column", "Entire column")],
                }],
                json!({"shift": "down"}),
            ),
            "insertPaste" => Dialog::form(
                "insertPaste",
                "Insert Paste",
                "home.insertCopiedCells",
                vec![Choice { key: "shift", label: "Insert", options: vec![("right", "Shift cells right"), ("down", "Shift cells down")] }],
                json!({"shift": "down"}),
            ),
            "deleteCells" => Dialog::form(
                "deleteCells",
                "Delete",
                "home.deleteCells",
                vec![Choice {
                    key: "shift",
                    label: "Delete",
                    options: vec![("left", "Shift cells left"), ("up", "Shift cells up"), ("row", "Entire row"), ("column", "Entire column")],
                }],
                json!({"shift": "up"}),
            ),
            "rowHeight" => Dialog::form(
                "rowHeight",
                "Row Height",
                "home.rowHeight",
                vec![Number { key: "height", label: "Row height (pixels):" }],
                json!({"height": app.session.active().and_then(|d| d.wb.active().map(|s| s.row_height(d.selection.active.row))).unwrap_or(20.0)}),
            ),
            "columnWidth" => Dialog::form(
                "columnWidth",
                "Column Width",
                "home.columnWidth",
                vec![Number { key: "chars", label: "Column width (characters):" }],
                json!({"chars": app.session.active().and_then(|d| d.wb.active().map(|s| gridcraft_engine::cmd::format::points_to_chars(s.col_width(d.selection.active.col) as f64))).unwrap_or(8.43)}),
            ),
            "defaultWidth" => Dialog::form(
                "defaultWidth",
                "Standard Width",
                "home.defaultWidth",
                vec![Number { key: "width", label: "Standard column width (pixels):" }],
                json!({"width": 64}),
            ),
            "renameSheet" => Dialog::form(
                "renameSheet",
                "Rename Sheet",
                "sheet.rename",
                vec![Text { key: "name", label: "Name:" }],
                json!({"name": app.session.active().and_then(|d| d.wb.active().map(|s| s.name.clone()))}),
            ),
            "moveSheet" => {
                let sheets: Vec<String> = app.session.active().map(|d| d.wb.sheets.iter().map(|s| s.name.clone()).collect()).unwrap_or_default();
                let mut d = Dialog::form(
                    "moveSheet",
                    "Move or Copy",
                    "sheet.move",
                    vec![
                        Number { key: "to", label: "Before sheet (position, 0 = first):" },
                        Check { key: "copy", label: "Create a copy" },
                        Note(format!("Sheets: {}", sheets.join(", "))),
                    ],
                    json!({"to": 0, "copy": false}),
                );
                if let Some(s) = params.get("sheet") {
                    d.values.insert("sheet".into(), s.clone());
                }
                d
            }
            "protectSheet" => Dialog::form(
                "protectSheet",
                "Protect Sheet",
                "review.protectSheet",
                vec![
                    Password { key: "password", label: "Password (optional):" },
                    Note("Allow all users of this sheet to:".into()),
                    Check { key: "formatCells", label: "Format cells" },
                    Check { key: "formatColumns", label: "Format columns" },
                    Check { key: "formatRows", label: "Format rows" },
                    Check { key: "insertRows", label: "Insert rows" },
                    Check { key: "deleteRows", label: "Delete rows" },
                    Check { key: "sort", label: "Sort" },
                    Check { key: "autofilter", label: "Use AutoFilter" },
                ],
                json!({}),
            ),
            "allowEditRanges" => {
                let list = app.session.run("review.allowEditRanges", json!({})).unwrap_or(Json::Null);
                let rows: Vec<String> = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|r| {
                                let lock = match (r["password"].as_bool(), r["unlocked"].as_bool()) {
                                    (Some(true), Some(true)) => " (password, unlocked)",
                                    (Some(true), _) => " (password)",
                                    _ => "",
                                };
                                format!("{}: {}{lock}", r["title"].as_str().unwrap_or(""), r["range"].as_str().unwrap_or(""))
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let summary = if rows.is_empty() { "No ranges yet.".to_string() } else { rows.join("\n") };
                Dialog::form(
                    "allowEditRanges",
                    "Allow Edit Ranges",
                    "review.allowEditRanges",
                    vec![
                        Note(format!("Ranges that stay editable when the sheet is protected:\n{summary}")),
                        Choice {
                            key: "action",
                            label: "Action",
                            options: vec![("add", "New"), ("modify", "Modify"), ("delete", "Delete"), ("unlock", "Unlock with password")],
                        },
                        Text { key: "title", label: "Title:" },
                        Text { key: "range", label: "Refers to cells:" },
                        Text { key: "password", label: "Range password:" },
                        Text { key: "newTitle", label: "New title (Modify):" },
                        Check { key: "removePassword", label: "Remove the password (Modify)" },
                    ],
                    json!({"action": "add", "range": sel}),
                )
            }
            "protectWorkbook" => Dialog::form(
                "protectWorkbook",
                "Protect Structure and Windows",
                "review.protectWorkbook",
                vec![
                    Password { key: "password", label: "Password (optional):" },
                    Note("Protect workbook for: Structure (sheets can't be added, moved, renamed, hidden or deleted).".into()),
                ],
                json!({"on": true}),
            ),
            "unprotectWorkbook" => Dialog::form(
                "unprotectWorkbook",
                "Unprotect Workbook",
                "review.protectWorkbook",
                vec![Password { key: "password", label: "Password:" }],
                json!({"on": false}),
            ),
            "openPassword" => {
                // The file rides along in the values (`path` on the desktop, `name` and `base64`
                // on the web) and goes back to `file.open` with the password.
                let source = p("path").as_str().or(p("name").as_str()).map(str::to_string).unwrap_or_default();
                let file = std::path::Path::new(&source).file_name().and_then(|n| n.to_str()).unwrap_or("This workbook").to_string();
                // An empty password from the start, so OK on the untouched field says "incorrect"
                // rather than asking for the password again.
                let mut values = params.clone();
                values["password"] = json!("");
                Dialog::form(
                    "openPassword",
                    "Password",
                    "file.open",
                    vec![Note(format!("'{file}' is protected.")), Password { key: "password", label: "Password:" }],
                    values,
                )
            }
            "unprotectSheet" => Dialog::form(
                "unprotectSheet",
                "Unprotect Sheet",
                "review.unprotectSheet",
                vec![Password { key: "password", label: "Password:" }],
                json!({}),
            ),
            "unhideSheet" => {
                let opts: Vec<String> =
                    p("sheets").as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
                let first = opts.first().cloned().unwrap_or_default();
                let mut d = Dialog::form(
                    "unhideSheet",
                    "Unhide",
                    "sheet.unhide",
                    vec![Text { key: "sheet", label: "Unhide sheet:" }, Note(format!("Hidden: {}", opts.join(", ")))],
                    json!({"sheet": first}),
                );
                d.values.insert("sheet".into(), json!(first));
                d
            }
            "pasteSpecial" => Dialog::form(
                "pasteSpecial",
                "Paste Special",
                "edit.pasteSpecial",
                vec![
                    Choice {
                        key: "what",
                        label: "Paste",
                        options: vec![
                            ("all", "All"),
                            ("formulas", "Formulas (F)"),
                            ("values", "Values (V)"),
                            ("formats", "Formats (T)"),
                            ("comments", "Comments and notes (C)"),
                            ("validation", "Validation"),
                            ("allExceptBorders", "All except borders"),
                            ("columnWidths", "Column widths"),
                            ("formulasAndNumberFormats", "Formulas and number formats"),
                            ("valuesAndNumberFormats", "Values and number formats"),
                        ],
                    },
                    Choice {
                        key: "operation",
                        label: "Operation",
                        options: vec![("none", "None"), ("add", "Add"), ("subtract", "Subtract"), ("multiply", "Multiply"), ("divide", "Divide")],
                    },
                    Check { key: "skipBlanks", label: "Skip blanks" },
                    Check { key: "transpose", label: "Transpose (E)" },
                    Check { key: "link", label: "Paste Link" },
                ],
                json!({"what": "all", "operation": "none"}),
            ),
            "series" => Dialog::form(
                "series",
                "Series",
                "edit.fillSeries",
                vec![
                    Choice { key: "direction", label: "Series in", options: vec![("columns", "Columns"), ("rows", "Rows")] },
                    Choice { key: "type", label: "Type", options: vec![("linear", "Linear"), ("growth", "Growth"), ("date", "Date")] },
                    Choice {
                        key: "dateUnit",
                        label: "Date unit",
                        options: vec![("day", "Day"), ("weekday", "Weekday"), ("month", "Month"), ("year", "Year")],
                    },
                    Number { key: "step", label: "Step value:" },
                    Text { key: "stop", label: "Stop value:" },
                ],
                json!({"direction": "columns", "type": "linear", "dateUnit": "day", "step": 1}),
            ),
            "comment" => {
                let threaded = p("threaded").as_bool().unwrap_or(true);
                let existing = app
                    .session
                    .active()
                    .and_then(|d| d.wb.active().and_then(|s| s.comments.get(&d.selection.active).map(|c| c.text.clone())))
                    .unwrap_or_default();
                Dialog::form(
                    if threaded { "comment" } else { "note" },
                    if threaded { "New Comment" } else { "New Note" },
                    if threaded { "review.newComment" } else { "review.newNote" },
                    vec![Text { key: "text", label: if threaded { "Start a conversation:" } else { "Note:" } }],
                    json!({"text": existing}),
                )
            }
            // Asks before a web or e-mail link leaves the workbook; OK runs `link.open`, which
            // refuses anything but http, https and mailto.
            "openLink" => {
                let url = params.get("url").and_then(Json::as_str).unwrap_or_default().to_string();
                Dialog::form(
                    "openLink",
                    "Open Hyperlink",
                    "link.open",
                    vec![Note(format!(
                        "This workbook asks to open the address below. Links can lead to harmful sites or start other programs: open it only if you trust the workbook.\n\n{url}"
                    ))],
                    json!({"url": url}),
                )
            }
            "insertLink" => Dialog::form(
                "insertLink",
                "Insert Hyperlink",
                "insert.link",
                vec![
                    Text { key: "text", label: "Display:" },
                    Text { key: "target", label: "Link to (web address, e-mail or Sheet!A1):" },
                    Text { key: "tooltip", label: "ScreenTip:" },
                ],
                json!({"target": "https://"}),
            ),
            "sparkline" => Dialog::form(
                "sparkline",
                "Create Sparklines",
                "insert.sparkline",
                vec![
                    Text { key: "range", label: "Data Range:" },
                    Text { key: "location", label: "Location Range:" },
                    Check { key: "markers", label: "Markers" },
                ],
                json!({"range": sel, "location": active, "type": p("type")}),
            ),
            "headerFooter" => Dialog::form(
                "headerFooter",
                "Header & Footer",
                "pageLayout.headerFooter",
                vec![
                    Text { key: "header", label: "Header (&L left, &C center, &R right; &P page, &N pages, &D date, &A sheet):" },
                    Text { key: "footer", label: "Footer:" },
                ],
                json!({"header": "", "footer": "&CPage &P of &N"}),
            ),
            "pageSetup" => {
                // Opens on the active sheet's saved margins, so OK without edits rewrites them unchanged.
                let m = app.session.active().and_then(|d| d.wb.active()).map(|s| s.print.margins).unwrap_or([0.7, 0.7, 0.75, 0.75, 0.3, 0.3]);
                // Round off f32 noise so the form shows 0.1, not 0.10000000149.
                let [left, right, top, bottom, header, footer] = m.map(|v| (f64::from(v) * 1e4).round() / 1e4);
                Dialog::form(
                    "pageSetup",
                    "Page Setup",
                    "pageLayout.margins",
                    vec![
                        Number { key: "left", label: "Left margin (in):" },
                        Number { key: "right", label: "Right margin (in):" },
                        Number { key: "top", label: "Top margin (in):" },
                        Number { key: "bottom", label: "Bottom margin (in):" },
                        Number { key: "header", label: "Header (in):" },
                        Number { key: "footer", label: "Footer (in):" },
                    ],
                    json!({"left": left, "right": right, "top": top, "bottom": bottom, "header": header, "footer": footer}),
                )
            }
            "textToColumns" => Dialog::form(
                "textToColumns",
                "Convert Text to Columns",
                "data.textToColumns",
                vec![
                    Check { key: "tab", label: "Tab" },
                    Check { key: "semicolon", label: "Semicolon" },
                    Check { key: "comma", label: "Comma" },
                    Check { key: "space", label: "Space" },
                    Text { key: "other", label: "Other:" },
                    Check { key: "treatConsecutive", label: "Treat consecutive delimiters as one" },
                    Text { key: "destination", label: "Destination:" },
                ],
                json!({"tab": true, "comma": true, "destination": active}),
            ),
            "removeDuplicates" => Dialog::form(
                "removeDuplicates",
                "Remove Duplicates",
                "data.removeDuplicates",
                vec![Check { key: "header", label: "My data has headers" }, Text { key: "columnsText", label: "Columns (blank = all, e.g. A,C):" }],
                json!({"header": true}),
            ),
            "goalSeek" => Dialog::custom("goalSeek", "Goal Seek", json!({"set": active, "to": "0", "changing": ""})),
            "forecastSheet" => Dialog::form(
                "forecastSheet",
                "Create Forecast Worksheet",
                "data.forecastSheet",
                vec![
                    Text { key: "timeline", label: "Timeline range (blank: first column of the selection):" },
                    Text { key: "values", label: "Values range (blank: second column of the selection):" },
                    Text { key: "end", label: "Forecast end (date or number; blank: a third of the history):" },
                    Text { key: "forecastStart", label: "Forecast start (a date or number in the history; blank: after the last point):" },
                    Check { key: "confidenceInterval", label: "Confidence interval" },
                    Text { key: "confidence", label: "Confidence level (%):" },
                    Text { key: "seasonality", label: "Seasonality (auto, 0 for none, or a season length):" },
                    Choice {
                        key: "fillMissing",
                        label: "Fill missing points using",
                        options: vec![("interpolate", "Interpolation"), ("zeros", "Zeros")],
                    },
                    Choice {
                        key: "aggregate",
                        label: "Aggregate duplicates using",
                        options: vec![
                            ("average", "Average"),
                            ("count", "Count"),
                            ("counta", "CountA"),
                            ("max", "Max"),
                            ("median", "Median"),
                            ("min", "Min"),
                            ("sum", "Sum"),
                        ],
                    },
                    Check { key: "statistics", label: "Include forecast statistics" },
                    Choice { key: "chart", label: "Chart", options: vec![("line", "Line chart"), ("column", "Column chart")] },
                ],
                json!({"confidenceInterval": true, "confidence": "95", "seasonality": "auto", "fillMissing": "interpolate", "aggregate": "average", "chart": "line"}),
            ),
            "analysisToolPak" => {
                // Opened with a known `tool`, it goes straight to that tool's form.
                let tool = params.get("tool").and_then(Json::as_str).unwrap_or("");
                let mut d = Dialog::custom("analysisToolPak", "Data Analysis", json!({"tool": "", "pick": tool}));
                if gridcraft_engine::cmd::analysis::TOOLS.iter().any(|(id, _)| *id == tool) {
                    d.values = toolpak_defaults(&sel, tool);
                }
                d.command = Some("data.analysisToolPak");
                d
            }
            "solver" => {
                // Starts from the sheet's saved model, else the active cell as objective.
                let saved = app.session.active().and_then(|doc| gridcraft_engine::cmd::solver::saved_model(&doc.wb, doc.wb.active_sheet));
                let mut values = json!({"objective": active, "goal": "max", "value": "0", "variables": "", "constraints": [], "nonNegative": true, "method": "grg"});
                if let (Some(m), Some(v)) = (saved.as_ref().and_then(Json::as_object), values.as_object_mut()) {
                    for (k, x) in m {
                        v.insert(k.clone(), if k == "value" { json!(x.to_string()) } else { x.clone() });
                    }
                }
                if let Some(v) = values.as_object_mut() {
                    v.insert("newOp".into(), json!("<="));
                }
                let mut d = Dialog::custom("solver", "Solver Parameters", values);
                d.command = Some("data.solver");
                d
            }
            "solverResults" => Dialog::custom("solverResults", "Solver Results", params.clone()),
            "subtotal" => Dialog::form(
                "subtotal",
                "Subtotal",
                "data.subtotal",
                vec![
                    Text { key: "groupBy", label: "At each change in (column letter):" },
                    Choice {
                        key: "function",
                        label: "Use function",
                        options: vec![
                            ("sum", "Sum"),
                            ("count", "Count"),
                            ("average", "Average"),
                            ("max", "Max"),
                            ("min", "Min"),
                            ("product", "Product"),
                        ],
                    },
                    Text { key: "columnsText", label: "Add subtotal to (columns, e.g. C,D):" },
                ],
                json!({"groupBy": "A", "function": "sum"}),
            ),
            "zoom" => Dialog::form(
                "zoom",
                "Zoom",
                "view.zoom",
                vec![
                    Choice {
                        key: "preset",
                        label: "Magnification",
                        options: vec![("200", "200%"), ("100", "100%"), ("75", "75%"), ("50", "50%"), ("25", "25%")],
                    },
                    Number { key: "percent", label: "Custom (%):" },
                ],
                json!({"percent": app.session.active().and_then(|d| d.wb.active().map(|s| s.zoom)).unwrap_or(100)}),
            ),
            "defineName" => {
                // Name Manager › Edit… passes the name being edited (`oldName`, `oldScope`) and
                // its current fields; New… and Define Name start from the selection.
                let editing = params.get("oldName").and_then(Json::as_str).is_some();
                let selection = app.session.active().and_then(|d| {
                    d.wb.active().map(|s| format!("={}!{}", gridcraft_engine::formula::quote_sheet(&s.name), abs(&d.selection.current())))
                });
                let mut defaults = json!({"scope": "Workbook", "refersTo": selection});
                for key in ["name", "scope", "comment", "refersTo", "oldName", "oldScope"] {
                    if let Some(v) = params.get(key).filter(|v| v.is_string()) {
                        defaults[key] = v.clone();
                    }
                }
                Dialog::form(
                    "defineName",
                    if editing { "Edit Name" } else { "New Name" },
                    "formulas.defineName",
                    vec![
                        Text { key: "name", label: "Name:" },
                        Text { key: "scope", label: "Scope (Workbook or sheet name):" },
                        Text { key: "comment", label: "Comment:" },
                        Text { key: "refersTo", label: "Refers to:" },
                    ],
                    defaults,
                )
            }
            "dataValidation" => Dialog::form(
                "dataValidation",
                "Data Validation",
                "data.validation",
                data_validation_fields("any", "between"),
                validation_defaults(app),
            ),
            "cfQuick" => {
                let ty = p("type").as_str().unwrap_or("cellIs").to_string();
                let title = p("title").as_str().unwrap_or("Conditional Formatting").to_string();
                let mut fields = vec![];
                match ty.as_str() {
                    "cellIs" => {
                        fields.push(Text { key: "value", label: "Format cells that are (value):" });
                        if p("operator").as_str() == Some("between") {
                            fields.push(Text { key: "value2", label: "and:" });
                        }
                    }
                    "containsText" => fields.push(Text { key: "text", label: "Format cells that contain the text:" }),
                    "timePeriod" => fields.push(Choice {
                        key: "period",
                        label: "Format cells that contain a date occurring:",
                        options: vec![
                            ("yesterday", "Yesterday"),
                            ("today", "Today"),
                            ("tomorrow", "Tomorrow"),
                            ("last7Days", "In the last 7 days"),
                            ("lastWeek", "Last week"),
                            ("thisWeek", "This week"),
                            ("nextWeek", "Next week"),
                            ("lastMonth", "Last month"),
                            ("thisMonth", "This month"),
                            ("nextMonth", "Next month"),
                        ],
                    }),
                    _ => fields.push(Text { key: "formula", label: "Format values where this formula is true:" }),
                }
                fields.push(Choice {
                    key: "preset",
                    label: "with",
                    options: vec![
                        ("lightRedFillDarkRedText", "Light Red Fill with Dark Red Text"),
                        ("yellowFillDarkYellowText", "Yellow Fill with Dark Yellow Text"),
                        ("greenFillDarkGreenText", "Green Fill with Dark Green Text"),
                        ("lightRedFill", "Light Red Fill"),
                        ("redText", "Red Text"),
                        ("redBorder", "Red Border"),
                    ],
                });
                let mut d = Dialog::form(
                    "cfQuick",
                    &title,
                    "home.conditionalFormat",
                    fields,
                    json!({"preset": "lightRedFillDarkRedText", "period": "today"}),
                );
                d.values.insert("type".into(), json!(ty));
                d.values.insert("operator".into(), p("operator"));
                d
            }
            "formatCells" => {
                let mut d = Dialog::custom("formatCells", "Format Cells", json!({}));
                d.tab = p("tab").as_str().unwrap_or("Number").to_string();
                if let Ok(st) = app.session.run("cell.get", json!({})) {
                    d.values.insert("style".into(), st["style"].clone());
                }
                d
            }
            "insertFunction" => Dialog::custom("insertFunction", "Formula Builder", json!({"category": "Most Recently Used"})),
            "find" => {
                let mut d = Dialog::custom("find", "Find and Replace", json!({"what": "", "with": "", "within": "sheet", "lookIn": "formulas"}));
                d.tab = if p("replace").as_bool() == Some(true) { "Replace".into() } else { "Find".into() };
                d
            }
            "goTo" => Dialog::custom("goTo", "Go To", json!({"reference": ""})),
            "pivotCalculatedField" => Dialog::form(
                "pivotCalculatedField",
                "Insert Calculated Field",
                "pivot.calculatedField",
                vec![Text { key: "name", label: "Name:" }, Text { key: "formula", label: "Formula:" }],
                json!({"pivot": p("pivot"), "name": p("name"), "formula": p("formula")}),
            ),
            "goToSpecial" => Dialog::form(
                "goToSpecial",
                "Go To Special",
                "edit.goToSpecial",
                vec![Choice {
                    key: "kind",
                    label: "Select",
                    options: vec![
                        ("blanks", "Blanks"),
                        ("constants", "Constants"),
                        ("formulas", "Formulas"),
                        ("notes", "Notes"),
                        ("errors", "Errors"),
                        ("numbers", "Numbers"),
                        ("text", "Text"),
                        ("currentRegion", "Current region"),
                        ("lastCell", "Last cell"),
                        ("visible", "Visible cells only"),
                        ("conditionalFormats", "Conditional formats"),
                        ("dataValidation", "Data validation"),
                    ],
                }],
                json!({"kind": "blanks"}),
            ),
            "sort" => Dialog::custom("sort", "Sort", json!({"header": true, "levels": [{"column": "A", "order": "asc"}]})),
            "nameManager" => Dialog::custom("nameManager", "Name Manager", json!({})),
            "manageRules" => Dialog::custom("manageRules", "Conditional Formatting Rules Manager", json!({})),
            "commandSearch" => Dialog::custom("commandSearch", "Search Commands", json!({})),
            "agents" => Dialog::custom("agents", "Agent Control", json!({})),
            "about" => {
                let mut d = Dialog::custom("about", "About GridCraft", json!({}));
                d.tab = match p("tab").as_str() {
                    Some(t @ ("Contributors" | "Models")) => t.to_string(),
                    _ => "About".into(),
                };
                d
            }
            "recommendedCharts" => {
                // Insert › Recommended Charts: chart types for the selected data, with previews.
                let recs = match app.session.run("insert.chartRecommendations", json!({})) {
                    Ok(r) => r,
                    Err(e) => {
                        app.message = Some(("Insert Chart".into(), crate::clean_error(&e)));
                        return None;
                    }
                };
                let mut d = Dialog::custom("recommendedCharts", "Insert Chart", json!({}));
                d.tab = if p("tab").as_str() == Some("All Charts") { "All Charts".into() } else { "Recommended Charts".into() };
                d.result = Some(recs);
                select_chart(&mut d, 0);
                d
            }
            "selectData" => {
                // Chart Design › Select Data: data range, series to keep, category axis labels.
                let id = p("chart").as_u64();
                let found = app.session.active().and_then(|doc| {
                    let si = doc.wb.active_sheet;
                    let sh = doc.wb.sheet(si)?;
                    let ch = sh.charts.iter().find(|c| Some(u64::from(c.id)) == id)?;
                    let data = gridcraft_chart::resolve(&doc.wb, si, ch);
                    let names: Vec<String> = data.series.iter().map(|x| x.name.clone()).collect();
                    let cats = ch.series.first().and_then(|x| x.categories.clone()).unwrap_or_default();
                    Some((ch.source.clone().unwrap_or_default(), cats, names))
                });
                let Some((range, cats, names)) = found else {
                    app.message = Some(("Select Data".into(), "Select a chart first.".into()));
                    return None;
                };
                let eq = |t: &str| if t.is_empty() { String::new() } else { format!("={t}") };
                Dialog::custom(
                    "selectData",
                    "Select Data Source",
                    json!({"chart": id, "range": eq(&range), "range0": eq(&range), "categories": eq(&cats), "categories0": eq(&cats), "names": names, "remove": []}),
                )
            }
            "journal" => Dialog::custom("journal", "Action Journal", json!({})),
            "macros" => {
                let mut d = Dialog::custom("macros", "Macros", json!({}));
                d.result = app.session.run("view.macros", json!({})).ok();
                d
            }
            "statistics" => {
                let mut d = Dialog::custom("statistics", "Workbook Statistics", json!({}));
                d.result = app.session.run("review.workbookStatistics", json!({})).ok();
                d
            }
            "accessibility" => {
                let mut d = Dialog::custom("accessibility", "Accessibility", json!({}));
                d.result = app.session.run("review.checkAccessibility", json!({})).ok();
                d
            }
            "errorChecking" => {
                let mut d = Dialog::custom("errorChecking", "Error Checking", json!({}));
                d.result = app.session.run("formulas.errorChecking", json!({})).ok();
                d
            }
            "evaluateFormula" => {
                let mut d = Dialog::custom("evaluateFormula", "Evaluate Formula", json!({}));
                d.result = app.session.run("formulas.evaluateFormula", json!({})).ok();
                d
            }
            "spelling" => {
                let mut d = Dialog::custom("spelling", "Spelling", json!({}));
                d.result = Some(app.session.run("review.spelling", json!({})).unwrap_or_else(|e| json!({"error": e})));
                d
            }
            "chartTitle" => Dialog::form(
                "chartTitle",
                "Chart Title",
                "chart.set",
                vec![
                    Text { key: "title", label: "Title:" },
                    Text { key: "xTitle", label: "Horizontal axis title:" },
                    Text { key: "yTitle", label: "Vertical axis title:" },
                ],
                json!({"chart": p("chart")}),
            ),
            "insertPicture" => {
                let placement = params.get("placement").and_then(Json::as_str).unwrap_or("overCells");
                let mut dialog = Dialog::form(
                    "insertPicture",
                    if placement == "cell" { "Place Picture in Cell" } else { "Place Picture over Cells" },
                    "insert.picture",
                    vec![],
                    json!({"path": "", "placement": placement, "at": active}),
                );
                dialog.picture_target = app.view_key();
                dialog
            }
            "powerPivot" => {
                // Excel's Power Pivot window: the File/Home/Design/Advanced tab strip, the
                // Data View or Diagram View canvas, and floating dialogs for the commands that
                // Excel opens from the ribbon. The model lives in the engine session, so the
                // dialog reads it back through `dataModel.status`.
                let status = app.session.run("dataModel.status", json!({})).unwrap_or(Json::Null);
                let mut dialog = Dialog::custom(
                    "powerPivot",
                    "Power Pivot",
                    json!({
                        "tab": "Home",
                        "view": "Data",
                        "overlay": "",
                        "path": p("path").as_str().unwrap_or_default(),
                        "formula": "",
                        "column": "",
                        "columnIndex": 0,
                        "measureIndex": -1,
                        "filterColumn": "",
                        "filterValue": "",
                        "calcArea": true,
                        "calcMode": "Automatic",
                        "showHidden": false,
                        "showImplicit": false,
                        "resultKind": "",
                        "resultText": "",
                        "error": "",
                        "model": status,
                        "diagramZoom": 1.0,
                        "diagramFit": false,
                        "tableIndex": 0,
                        "showFunctions": false,
                        "fnCategory": "All",
                        "fnSearch": "",
                        "measureDialogMode": "Add",
                        "newMeasureName": "",
                        "newMeasureTable": "",
                        "newMeasureFormula": "",
                        "newMeasureFormat": "",
                        "relIndex": -1,
                        "relFromTable": "",
                        "relFromColumn": "",
                        "relToTable": "",
                        "relToColumn": "",
                        "relBidirectional": false,
                    }),
                );
                // Excel's Data View shows the selected table's rows; status only carries counts,
                // so fetch the first table as a table expression and select its first column.
                if let Some(table) = status.get("tables").and_then(Json::as_array).and_then(|tables| tables.first()) {
                    if let Some(column) = table
                        .get("columns")
                        .and_then(Json::as_array)
                        .and_then(|columns| columns.first())
                        .and_then(|column| column.get("name"))
                        .and_then(Json::as_str)
                    {
                        dialog.values.insert("column".into(), json!(column));
                    }
                    if let Some(table_name) = table.get("name").and_then(Json::as_str) {
                        dialog.values.insert("newMeasureTable".into(), json!(table_name));
                        if let Ok(data) = app.session.run("dataModel.evaluate", json!({"formula": table_name, "table": true})) {
                            dialog.values.insert("tableData".into(), data);
                        }
                    }
                }
                dialog
            }
            "pickList" => Dialog::custom("pickList", "Pick From List", json!({})),
            "saveCopy" => Dialog::custom("saveCopy", "Save a Copy", json!({"format": p("format")})),
            "saveChanges" => Dialog::custom("saveChanges", "Save Changes?", json!({"title": p("title")})),
            "start" => Dialog::custom("start", "GridCraft", json!({})),
            "comments" => {
                app.grid.pane = None;
                app.run_or_alert("review.showComments", json!({"on": true}));
                return None;
            }
            "watch" | "selectionPane" | "formatChart" => {
                app.grid.pane = Some(
                    match name {
                        "watch" => "watch",
                        "selectionPane" => "selection",
                        _ => "formatChart",
                    }
                    .into(),
                );
                return None;
            }
            _ => return None,
        };
        Some(d)
    }
}

/// Initial values of the Data Validation form: the rule stored on the active cell, so OK without
/// edits keeps it; a plain list rule when the cell has none.
fn validation_defaults(app: &SheetApp) -> Json {
    let mut v = json!({"type": "list", "operator": "between", "ignoreBlank": true, "dropdown": true, "errorStyle": "stop"});
    let Some(doc) = app.session.active() else { return v };
    let Some(dv) = doc.wb.active().and_then(|s| s.validations.iter().find(|d| d.ranges.iter().any(|r| r.contains(doc.selection.active)))) else {
        return v;
    };
    // The enums serialize as their variant names; the command takes them in camelCase.
    let camel = |x: Json| {
        let name = x.as_str().unwrap_or_default();
        let mut chars = name.chars();
        chars.next().map(|c| c.to_lowercase().chain(chars).collect::<String>()).unwrap_or_default()
    };
    v["type"] = json!(camel(json!(dv.kind)));
    v["operator"] = json!(camel(json!(dv.op)));
    v["errorStyle"] = json!(camel(json!(dv.error_style)));
    v["formula1"] = json!(dv.f1);
    if let Some(f2) = &dv.f2 {
        v["formula2"] = json!(f2);
    }
    v["ignoreBlank"] = json!(dv.allow_blank);
    v["dropdown"] = json!(dv.in_cell_dropdown);
    v["inputTitle"] = json!(dv.input_title);
    v["inputMessage"] = json!(dv.input_message);
    v["errorTitle"] = json!(dv.error_title);
    v["errorMessage"] = json!(dv.error_message);
    v
}

fn abs(r: &gridcraft_engine::core::RangeRef) -> String {
    let a = |c: gridcraft_engine::core::CellRef| format!("${}${}", gridcraft_engine::core::col_to_letters(c.col), c.row + 1);
    if r.is_single() { a(r.start) } else { format!("{}:{}", a(r.start), a(r.end)) }
}

/// The criterion controls in Excel's Settings tab depend on both Allow and Data.
/// `formula1` is the sole source for a list or custom rule; `formula2` is a
/// second bound only for Between and Not between.
fn data_validation_fields(kind: &str, operator: &str) -> Vec<Field> {
    use Field::*;
    let mut fields = vec![Choice {
        key: "type",
        label: "Allow",
        options: vec![
            ("any", "Any value"),
            ("whole", "Whole number"),
            ("decimal", "Decimal"),
            ("list", "List"),
            ("date", "Date"),
            ("time", "Time"),
            ("textLength", "Text length"),
            ("custom", "Custom"),
        ],
    }];
    let two_bounds = matches!(operator, "between" | "notBetween");
    match kind {
        "list" => {
            fields.push(Text { key: "formula1", label: "Source:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
            fields.push(Check { key: "dropdown", label: "In-cell dropdown" });
        }
        "custom" => {
            fields.push(Text { key: "formula1", label: "Formula:" });
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        "whole" | "decimal" | "date" | "time" | "textLength" => {
            fields.push(Choice {
                key: "operator",
                label: "Data",
                options: vec![
                    ("between", "between"),
                    ("notBetween", "not between"),
                    ("equal", "equal to"),
                    ("notEqual", "not equal to"),
                    ("greater", "greater than"),
                    ("less", "less than"),
                    ("greaterOrEqual", "greater than or equal to"),
                    ("lessOrEqual", "less than or equal to"),
                ],
            });
            let lower = matches!(operator, "greater" | "greaterOrEqual");
            let (first, second) = match kind {
                "date" => (
                    if two_bounds || lower {
                        "Start date:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End date:"
                    } else {
                        "Date:"
                    },
                    "End date:",
                ),
                "time" => (
                    if two_bounds || lower {
                        "Start time:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "End time:"
                    } else {
                        "Time:"
                    },
                    "End time:",
                ),
                _ => (
                    if two_bounds || lower {
                        "Minimum:"
                    } else if matches!(operator, "less" | "lessOrEqual") {
                        "Maximum:"
                    } else {
                        "Value:"
                    },
                    "Maximum:",
                ),
            };
            fields.push(Text { key: "formula1", label: first });
            if two_bounds {
                fields.push(Text { key: "formula2", label: second });
            }
            fields.push(Check { key: "ignoreBlank", label: "Ignore blank" });
        }
        _ => {}
    }
    fields.push(Text { key: "inputMessage", label: "Input message:" });
    fields.push(Choice {
        key: "errorStyle",
        label: "Error style",
        options: vec![("stop", "Stop"), ("warning", "Warning"), ("information", "Information")],
    });
    fields.push(Text { key: "errorMessage", label: "Error message:" });
    fields
}

fn data_validation_form(ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let kind = d.values.get("type").and_then(Json::as_str).unwrap_or("any");
    let operator = d.values.get("operator").and_then(Json::as_str).unwrap_or("between");
    d.fields = data_validation_fields(kind, operator);
    form_fields(ui, d);
    ok_cancel(ui, confirm, open);
}

/// Builds the command params from form values for dialogs that need translation.
fn build_params(d: &Dialog) -> Json {
    let mut v = Json::Object(d.values.clone());
    match d.name.as_str() {
        "dataValidation" => {
            let kind = d.values.get("type").and_then(Json::as_str).unwrap_or("any");
            let operator = d.values.get("operator").and_then(Json::as_str).unwrap_or("between");
            if let Some(m) = v.as_object_mut() {
                if !matches!(operator, "between" | "notBetween") || matches!(kind, "any" | "list" | "custom") {
                    m.remove("formula2");
                }
                if matches!(kind, "any" | "list" | "custom") {
                    m.remove("operator");
                }
                if kind == "any" {
                    m.remove("formula1");
                }
                if kind != "list" {
                    m.remove("dropdown");
                }
            }
        }
        "textToColumns" => {
            let mut delims = Vec::new();
            for (k, c) in [("tab", "\t"), ("semicolon", ";"), ("comma", ","), ("space", " ")] {
                if d.values.get(k).and_then(Json::as_bool) == Some(true) {
                    delims.push(json!(c));
                }
            }
            v["delimiters"] = Json::Array(delims);
        }
        "removeDuplicates" | "subtotal" => {
            if let Some(t) = d.values.get("columnsText").and_then(Json::as_str).filter(|t| !t.trim().is_empty()) {
                v["columns"] = Json::Array(t.split(',').map(|c| json!(c.trim().to_ascii_uppercase())).collect());
            }
        }
        "allowEditRanges" => {
            if let Some(m) = v.as_object_mut() {
                // Blank fields mean "unchanged"; the check box clears a password.
                m.retain(|_, x| x.as_str().is_none_or(|s| !s.trim().is_empty()));
                if m.remove("removePassword").and_then(|x| x.as_bool()) == Some(true) {
                    m.insert("password".into(), json!(""));
                }
            }
        }
        "zoom" => {
            if let Some(p) = d.values.get("preset").and_then(Json::as_str)
                && d.values.get("percent").is_none()
            {
                v["percent"] = json!(p.parse::<f64>().unwrap_or(100.0));
            }
        }
        "cfQuick" => {
            let mut rule = Map::new();
            for (k, val) in &d.values {
                rule.insert(k.clone(), val.clone());
            }
            v = json!({"rule": Json::Object(rule)});
        }
        "series" => {
            if let Some(s) = d.values.get("stop").and_then(Json::as_str).and_then(|s| s.parse::<f64>().ok()) {
                v["stop"] = json!(s);
            } else if let Some(m) = v.as_object_mut() {
                m.remove("stop");
            }
        }
        "defineName" => {
            if let Some(m) = v.as_object_mut() {
                for key in ["scope", "oldScope"] {
                    if m.get(key).and_then(Json::as_str) == Some("Workbook") {
                        m.remove(key);
                    }
                }
            }
        }
        _ => {}
    }
    v
}

pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    let Some(mut d) = app.dialog.take() else { return };
    if d.name == "pasteSpecial" {
        paste_special_shortcuts(&mut d, ctx);
    }
    let mut open = true;
    let mut win_open = true;
    let mut confirm = false;
    let width = match d.name.as_str() {
        "formatCells" => 560.0,
        "about" => 660.0,
        "insertFunction" | "commandSearch" | "nameManager" | "manageRules" | "journal" | "agents" | "macros" => 520.0,
        "recommendedCharts" => 600.0,
        "analysisToolPak" => 420.0,
        "solver" => 460.0,
        "powerPivot" => 1320.0,
        _ => 380.0,
    };
    let window = egui::Window::new(app.ui.language.tr(&d.title))
        .id(egui::Id::new("dialog").with(&d.name))
        .collapsible(false)
        .resizable(d.name == "powerPivot")
        .default_width(width)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .open(&mut win_open);
    let window = match d.name.as_str() {
        "insertPicture" => window.default_height(240.0),
        "powerPivot" => window.default_height(740.0),
        _ => window,
    };
    window.show(ctx, |ui| {
            ui.set_min_width(width - 20.0);
            match d.name.as_str() {
                "formatCells" => format_cells(app, ui, &mut d, &mut confirm),
                "dataValidation" => data_validation_form(ui, &mut d, &mut confirm, &mut open),
                "insertFunction" => insert_function(app, ui, &mut d, &mut confirm),
                "find" => find(app, ui, &mut d),
                "goTo" => go_to(app, ui, &mut d, &mut confirm),
                "sort" => sort(app, ui, &mut d, &mut confirm),
                "nameManager" => name_manager(app, ui, &mut d, &mut open),
                "manageRules" => manage_rules(app, ui),
                "recommendedCharts" => recommended_charts(app, ui, &mut d, &mut confirm, &mut open),
                "selectData" => select_data(app, ui, &mut d, &mut confirm, &mut open),
                "commandSearch" => command_search(app, ui, &mut d, &mut confirm),
                "goalSeek" => goal_seek(app, ui, &mut d, &mut confirm),
                "analysisToolPak" => analysis_toolpak(app, ui, &mut d, &mut confirm, &mut open),
                "solver" => solver(ui, &mut d, &mut confirm, &mut open),
                "solverResults" => solver_results(app, ui, &mut d, &mut open),
                "powerPivot" => power_pivot(app, ui, &mut d, &mut open),
                "spelling" => spelling(app, ui, &mut d, &mut open),
                "agents" => {
                    ui.label("GridCraft is fully drivable by agents. Every menu item, button and gesture is a command:");
                    ui.add_space(4.0);
                    ui.monospace("gridcraft --control 7979        # JSON-lines control channel\ngridcraft-cli mcp --connect 7979   # MCP bridged to this window\ngridcraft-cli mcp                   # headless MCP server");
                    ui.add_space(4.0);
                    ui.label(format!("{} engine commands are available. See docs/mcp.md and docs/control-protocol.md.", gridcraft_engine::command_specs().len()));
                }
                "about" => about(ui, &mut d),
                "journal" => {
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for (id, p) in app.session.journal.iter().rev().take(500) {
                            ui.monospace(format!("{id} {p}"));
                        }
                    });
                }
                "macros" => macros(ui, &mut d),
                "statistics" | "accessibility" | "errorChecking" | "evaluateFormula" | "comments" => {
                    let r = if d.name == "comments" {
                        app.session.active().and_then(|doc| doc.wb.active().map(|s| json!(s.comments.iter().map(|(c, m)| json!({"cell": c.a1(), "author": m.author, "text": m.text})).collect::<Vec<_>>())))
                    } else {
                        d.result.clone()
                    };
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        ui.monospace(serde_json::to_string_pretty(&r.unwrap_or(Json::Null)).unwrap_or_default());
                    });
                }
                "insertPicture" => {
                    // A private inbox dies with this dialog; late browser results
                    // cannot affect a later picture choice or another workbook.
                    let arrived = d.picture_inbox.as_ref().and_then(|inbox| {
                        inbox.lock().unwrap_or_else(std::sync::PoisonError::into_inner).pop()
                    });
                    if let Some((name, bytes)) = arrived {
                        d.picture_inbox = None;
                        if bytes.len() > 16 * 1024 * 1024 {
                            app.message = Some((d.title.clone(), "Choose a PNG or JPEG no larger than 16 MB.".into()));
                        } else {
                            d.values.remove("path");
                            d.values.insert("alt".into(), json!(name));
                            d.values.insert("base64".into(), json!(gridcraft_engine::io::base64_encode(&bytes)));
                        }
                    }
                    if app.services.pick_picture_async.is_some() {
                        let name = d.values.get("alt").and_then(Json::as_str).unwrap_or("No picture selected");
                        ui.label(name);
                    } else {
                        ui.label("Picture file path (PNG or JPEG):");
                        let mut path = d.values.get("path").and_then(Json::as_str).unwrap_or("").to_string();
                        if ui.text_edit_singleline(&mut path).changed() {
                            d.values.remove("base64");
                            d.values.insert("path".into(), json!(path));
                        }
                    }
                    if (app.services.pick_picture.is_some() || app.services.pick_picture_async.is_some()) && ui.button("Browse…").clicked() {
                        if let Some(start) = &app.services.pick_picture_async {
                            let inbox = crate::Inbox::default();
                            start(inbox.clone());
                            d.picture_inbox = Some(inbox);
                        } else if let Some(pick) = &app.services.pick_picture
                            && let Some(path) = pick()
                        {
                            d.values.remove("base64");
                            let alt = std::path::Path::new(&path).file_name().and_then(|name| name.to_str()).unwrap_or("");
                            d.values.insert("alt".into(), json!(alt));
                            d.values.insert("path".into(), json!(path));
                        }
                    }
                    ui.label("Description (optional):");
                    let mut alt = d.values.get("alt").and_then(Json::as_str).unwrap_or("").to_string();
                    if ui.text_edit_singleline(&mut alt).changed() {
                        d.values.insert("alt".into(), json!(alt));
                    }
                    if d.values.get("placement").and_then(Json::as_str) == Some("cell") {
                        ui.label(egui::RichText::new("Fits inside the selected cell and moves with its content.").small());
                    }
                    ok_cancel(ui, &mut confirm, &mut open);
                }
                "pickList" => {
                    let items: Vec<String> = app
                        .session
                        .active()
                        .and_then(|doc| {
                            let sh = doc.wb.active()?;
                            let a = doc.selection.active;
                            let mut seen = std::collections::BTreeSet::new();
                            let mut r = a.row;
                            while r > 0 && !sh.value(gridcraft_engine::core::CellRef::new(r - 1, a.col)).is_empty() {
                                r -= 1;
                                seen.insert(sh.value(gridcraft_engine::core::CellRef::new(r, a.col)).display());
                            }
                            Some(seen.into_iter().collect())
                        })
                        .unwrap_or_default();
                    for it in items {
                        if ui.button(&it).clicked() {
                            app.run_or_alert("cell.set", json!({"input": it}));
                            open = false;
                        }
                    }
                }
                "saveCopy" => {
                    let fmt = d.values.get("format").and_then(Json::as_str).unwrap_or("xlsx").to_string();
                    ui.label(format!("Save a copy as .{fmt}"));
                    let mut path = d.values.get("path").and_then(Json::as_str).unwrap_or("").to_string();
                    ui.text_edit_singleline(&mut path);
                    d.values.insert("path".into(), json!(path.clone()));
                    ok_cancel(ui, &mut confirm, &mut open);
                    if confirm {
                        let target = if path.is_empty() { app.services.pick_save.as_ref().and_then(|f| f(&format!("Copy.{fmt}"))) } else { Some(path) };
                        if let Some(t) = target {
                            let id = match fmt.as_str() {
                                "csv" => "file.exportCsv",
                                "html" => "file.exportHtml",
                                _ => "file.saveAs",
                            };
                            app.run_or_alert(id, json!({"path": t}));
                        }
                    }
                }
                "saveChanges" => {
                    ui.label(format!("Do you want to save the changes you made to {}?", d.values.get("title").and_then(Json::as_str).unwrap_or("this workbook")));
                    ui.horizontal(|ui| {
                        if ui.button("Don't Save").clicked() {
                            app.run_or_alert("file.close", json!({"force": true}));
                            open = false;
                        }
                        if ui.button("Cancel").clicked() {
                            open = false;
                        }
                        // File operations must include the cell the user is still editing (as the File > Save menu item already does).
                        if ui.button("Save").clicked() && app.commit_edit(0, 0, false, false) {
                            app.run_or_alert("file.save", json!({}));
                            if app.session.active().is_some_and(|x| !x.is_dirty()) {
                                app.run_or_alert("file.close", json!({"force": true}));
                            }
                            open = false;
                        }
                    });
                }
                "start" => {
                    ui.heading("Start a new workbook");
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Blank workbook").clicked() {
                            app.run_or_alert("file.new", json!({}));
                            open = false;
                        }
                        for (n, t) in gridcraft_engine::sample::SAMPLES {
                            if ui.button(*t).clicked() {
                                app.run_or_alert("file.new", json!({"sample": n}));
                                open = false;
                            }
                        }
                    });
                    ui.separator();
                    ui.label(egui::RichText::new("Recent").strong());
                    for path in app.ui.recent.clone() {
                        if ui.link(&path).clicked() {
                            app.open_path(&path);
                            open = false;
                        }
                    }
                    if ui.button("Open…").clicked() {
                        app.open_dialog("open", json!({}));
                        open = false;
                    }
                }
                _ => form(ui, &mut d, &mut confirm, &mut open),
            }
        });
    // Hyperlinks inside dialogs (About ▸ Contributors) open through the host's `open_url`, since
    // the native shell has no browser integration of its own.
    if let Some(open_url) = &app.services.open_url {
        let urls: Vec<String> = ctx.output_mut(|o| {
            let mut urls = vec![];
            o.commands.retain(|c| match c {
                egui::OutputCommand::OpenUrl(u) => {
                    urls.push(u.url.clone());
                    false
                }
                _ => true,
            });
            urls
        });
        for u in urls {
            open_url(&u);
        }
    }
    if ctx.input(|i| i.key_pressed(Key::Escape)) || !win_open {
        open = false;
    }
    if confirm && d.name == "insertPicture" && d.picture_target != app.view_key() {
        app.message = Some((d.title.clone(), "Return to the original worksheet to insert this picture, or cancel and choose another cell.".into()));
        confirm = false;
    }
    if confirm && let Some(cmd) = d.command {
        let params = build_params(&d);
        match app.run(cmd, params) {
            Ok(r) => {
                open = false;
                if d.name == "solver" {
                    app.open_dialog("solverResults", r);
                } else if d.name == "openPassword" {
                    let path = d.values.get("path").and_then(Json::as_str).map(str::to_string);
                    app.after_open(path.as_deref(), &r);
                }
            }
            // Data Analysis asks before replacing data in its output range.
            Err(e) if d.name == "analysisToolPak" && e.contains(gridcraft_engine::cmd::analysis::OVERWRITE_ERROR) => {
                d.values.insert("askOverwrite".into(), json!(true));
            }
            // A wrong password: say so, clear the field and stay open for another try.
            Err(e) if d.name == "openPassword" => {
                d.values.insert("password".into(), json!(""));
                app.message = Some((d.title.clone(), crate::clean_error(&e)));
            }
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    } else if confirm {
        open = false;
    }
    if let Some(child) = app.dialog.as_mut() {
        // The body opened another dialog (Name Manager › New…). It takes over; this one waits
        // behind it and comes back when it closes, as Excel returns to the Name Manager. Go To
        // hands over to Go To Special for good, as in Excel.
        if open && d.name != "goTo" {
            child.back = Some(Box::new(d));
        } else if child.back.is_none() {
            child.back = d.back.take();
        }
        return;
    }
    if open {
        app.dialog = Some(d);
    } else if let Some(back) = d.back.take() {
        app.dialog = Some(*back);
    }
}

/// Paste Special's legacy mnemonics select options; Enter still performs the paste.
fn paste_special_shortcuts(d: &mut Dialog, ctx: &egui::Context) {
    if ctx.text_edit_focused() {
        return;
    }
    let right_alt = ctx.input(|i| {
        i.key_down(Key::AltRight)
            || i.events.iter().any(|event| {
                matches!(event, egui::Event::Key { key: Key::AltRight, .. })
                    || matches!(event, egui::Event::Key { physical_key: Some(Key::AltRight), .. })
            })
    });
    let mac = ctx.os() == egui::os::OperatingSystem::Mac;
    let allowed = |m: egui::Modifiers| !m.ctrl && !m.command && !m.mac_cmd && !(m.alt && (mac || right_alt));
    let mnemonic = |key| match key {
        Key::T => Some('t'),
        Key::V => Some('v'),
        Key::F => Some('f'),
        Key::C => Some('c'),
        Key::E => Some('e'),
        _ => None,
    };
    let mut text_to_consume = Vec::new();
    for event in ctx.input(|i| i.events.clone()) {
        let egui::Event::Key { key, pressed: true, repeat, modifiers, .. } = event else { continue };
        if !allowed(modifiers) {
            continue;
        }
        let Some(letter) = mnemonic(key) else { continue };
        text_to_consume.push(letter);
        if repeat {
            continue;
        }
        if key == Key::E {
            let transpose = d.values.get("transpose").and_then(Json::as_bool).unwrap_or(false);
            d.values.insert("transpose".into(), json!(!transpose));
        } else {
            let what = match key {
                Key::T => "formats",
                Key::V => "values",
                Key::F => "formulas",
                _ => "comments",
            };
            d.values.insert("what".into(), json!(what));
        }
    }
    // Browsers can deliver Text before or after Key. Act only on Key, then remove
    // its matching text so one physical press cannot toggle Transpose twice.
    ctx.input_mut(|i| {
        i.events.retain(|event| match event {
            egui::Event::Key { key, modifiers, .. } => !(allowed(*modifiers) && mnemonic(*key).is_some()),
            egui::Event::Text(text) if text.len() == 1 => {
                let letter = text.chars().next().unwrap_or_default().to_ascii_lowercase();
                if let Some(index) = text_to_consume.iter().position(|&c| c == letter) {
                    text_to_consume.remove(index);
                    false
                } else {
                    true
                }
            }
            _ => true,
        });
    });
}

fn ok_cancel(ui: &mut egui::Ui, confirm: &mut bool, open: &mut bool) {
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let lang = crate::i18n::current(ui.ctx());
        let ok = ui.add(
            egui::Button::new(egui::RichText::new(format!("   {}   ", lang.tr("OK"))).color(Color32::WHITE))
                .fill(crate::theme::Tokens::get(ui.ctx()).accent),
        );
        if ok.clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
            *confirm = true;
        }
        if ui.button(format!(" {} ", lang.tr("Cancel"))).clicked() {
            *open = false;
        }
    });
}

fn form(ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    form_fields(ui, d);
    ok_cancel(ui, confirm, open);
}

/// Draws a dialog's `fields`, editing its values.
fn form_fields(ui: &mut egui::Ui, d: &mut Dialog) {
    egui::Grid::new("form").num_columns(1).spacing(vec2(8.0, 6.0)).show(ui, |ui| {
        for f in d.fields.clone() {
            match f {
                Field::Text { key, label } => {
                    ui.vertical(|ui| {
                        ui.label(label);
                        let mut s = d
                            .values
                            .get(key)
                            .map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| if v.is_null() { String::new() } else { v.to_string() }))
                            .unwrap_or_default();
                        let multiline = key == "text";
                        let r = if multiline {
                            ui.add(egui::TextEdit::multiline(&mut s).desired_rows(4).desired_width(320.0))
                        } else {
                            ui.add(egui::TextEdit::singleline(&mut s).desired_width(320.0))
                        };
                        if r.changed() {
                            d.values.insert(key.into(), json!(s));
                        }
                    });
                }
                Field::Password { key, label } => {
                    // Masked as typed (Excel shows dots); "Show password" reveals it on purpose.
                    ui.vertical(|ui| {
                        ui.label(label);
                        let mut s = d.values.get(key).and_then(Json::as_str).unwrap_or_default().to_string();
                        if ui.add(egui::TextEdit::singleline(&mut s).password(!d.show_password).desired_width(320.0)).changed() {
                            d.values.insert(key.into(), json!(s));
                        }
                        ui.checkbox(&mut d.show_password, "Show password");
                    });
                }
                Field::Number { key, label } => {
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let mut n = d.values.get(key).and_then(Json::as_f64).unwrap_or(0.0);
                        if ui.add(egui::DragValue::new(&mut n).speed(0.5)).changed() {
                            d.values.insert(key.into(), json!(n));
                        }
                    });
                }
                Field::Choice { key, label, options } => {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(label).strong());
                        let cur = d.values.get(key).and_then(Json::as_str).unwrap_or("").to_string();
                        if d.name == "dataValidation" {
                            let selected = options.iter().find(|(val, _)| *val == cur).map(|(_, text)| *text).unwrap_or("");
                            egui::ComboBox::from_id_salt(("dataValidation", key)).selected_text(selected).width(300.0).show_ui(ui, |ui| {
                                for (val, text) in options {
                                    if ui.selectable_label(cur == val, text).clicked() {
                                        d.set_value(key, json!(val));
                                    }
                                }
                            });
                        } else {
                            for (val, text) in options {
                                if ui.radio(cur == val, text).clicked() {
                                    d.set_value(key, json!(val));
                                }
                            }
                        }
                    });
                }
                Field::Check { key, label } => {
                    let mut b = d.values.get(key).and_then(Json::as_bool).unwrap_or(false);
                    if ui.checkbox(&mut b, label).changed() {
                        d.values.insert(key.into(), json!(b));
                    }
                }
                Field::Note(t) => {
                    ui.label(egui::RichText::new(t).small());
                }
            }
            ui.end_row();
        }
    });
}

/// Help ▸ About GridCraft: the app, its contributors and the AI models that helped. The credits
/// are compiled in (`crate::credits`, docs/contributors.md).
fn about(ui: &mut egui::Ui, d: &mut Dialog) {
    ui.horizontal(|ui| {
        for tab in ["About", "Contributors", "Models"] {
            if ui.selectable_label(d.tab == tab, tab).clicked() {
                d.tab = tab.to_string();
            }
        }
    });
    ui.separator();
    match d.tab.as_str() {
        "Contributors" => crate::credits::contributors_ui(ui),
        "Models" => crate::credits::models_ui(ui),
        _ => {
            ui.heading("GridCraft");
            ui.label(format!("Version {}", env!("CARGO_PKG_VERSION")));
            ui.add_space(4.0);
            ui.label("A clean-room, open-source, Rust-native spreadsheet. Part of ArtCraft.");
            ui.hyperlink_to("getartcraft.com/apps/gridcraft", "https://getartcraft.com/apps/gridcraft");
            ui.hyperlink_to("Join the ArtCraft Discord", "https://discord.gg/artcraft");
        }
    }
}

/// The Macros dialog: lists GridCraft's own recorded scripts and (if the file carries one) the
/// preserved VBA project. Mirrors Excel's Developer ▸ Macros list, but is honest that VBA is
/// preserved and not executed.
fn macros(ui: &mut egui::Ui, d: &mut Dialog) {
    ui.label("Automation in this workbook. GridCraft runs its own scripts; a file's VBA project is preserved but not executed.");
    ui.add_space(6.0);
    let items: Vec<Json> = d.result.as_ref().and_then(Json::as_array).cloned().unwrap_or_default();
    if items.is_empty() {
        ui.weak("This workbook has no scripts or macros.");
        return;
    }
    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
        for it in &items {
            let name = it.get("name").and_then(Json::as_str).unwrap_or("(unnamed)");
            let is_vba = it.get("kind").and_then(Json::as_str) == Some("vba");
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(name).strong());
                if is_vba {
                    ui.weak("— VBA project (preserved, not run)");
                } else {
                    let steps = it.get("steps").and_then(Json::as_u64).unwrap_or(0);
                    ui.weak(format!("— {steps} step{}", if steps == 1 { "" } else { "s" }));
                }
            });
        }
    });
}

fn format_cells(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    // `d.tab` stays the English name (it is what `open_dialog` and tests pass); only the drawn
    // labels are translated. The Fill tab is keyed by context: the Alignment tab's "Fill" is a
    // different word in some languages.
    let lang = app.ui.language;
    ui.horizontal(|ui| {
        for (tab, key) in [
            ("Number", "Number"),
            ("Alignment", "Alignment"),
            ("Font", "Font"),
            ("Border", "Border"),
            ("Fill", "Format Cells|Fill"),
            ("Protection", "Protection"),
        ] {
            if ui.selectable_label(d.tab == tab, lang.tr(key)).clicked() {
                d.tab = tab.to_string();
            }
        }
    });
    ui.separator();
    let mut st: gridcraft_engine::model::Style = d.values.get("style").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    let sample = app.session.active().and_then(|doc| doc.wb.active().map(|s| s.value(doc.selection.active))).unwrap_or_default();
    match d.tab.as_str() {
        "Number" => {
            let mut code = st.num_fmt.as_str().to_string();
            ui.columns(2, |cols| {
                cols[0].label(egui::RichText::new(lang.tr("Category:")).strong());
                for name in
                    ["General", "Number", "Currency", "Accounting", "Short Date", "Long Date", "Time", "Percentage", "Fraction", "Scientific", "Text"]
                {
                    let c = gridcraft_engine::cmd::format::format_code_for(name).to_string();
                    if cols[0].selectable_label(code == c, lang.tr(name)).clicked() {
                        code = c;
                    }
                }
                cols[1].label(egui::RichText::new(lang.tr("Sample")).strong());
                let preview = app.session.active().map(|doc| gridcraft_engine::display::format(&sample, &code, &doc.wb).text).unwrap_or_default();
                cols[1].label(egui::RichText::new(preview).font(theme::ui_font(15.0)));
                cols[1].add_space(8.0);
                cols[1].label(lang.tr("Type (custom format code):"));
                cols[1].text_edit_singleline(&mut code);
                for c in [
                    "0",
                    "0.00",
                    "#,##0",
                    "#,##0.00",
                    "#,##0;[Red]-#,##0",
                    "0%",
                    "0.00%",
                    "0.00E+00",
                    "# ?/?",
                    "m/d/yyyy",
                    "d-mmm-yy",
                    "mmmm d, yyyy",
                    "h:mm AM/PM",
                    "h:mm:ss",
                    "[h]:mm:ss",
                    "@",
                ] {
                    if cols[1].small_button(c).clicked() {
                        code = c.to_string();
                    }
                }
            });
            st.num_fmt = gridcraft_engine::model::NumFmt::new(&code);
        }
        "Alignment" => {
            use gridcraft_engine::model::{HAlign, VAlign};
            ui.label(egui::RichText::new(lang.tr("Horizontal")).strong());
            ui.horizontal_wrapped(|ui| {
                for (h, n) in [
                    (HAlign::General, "General"),
                    (HAlign::Left, "Left"),
                    (HAlign::Center, "Center"),
                    (HAlign::Right, "Right"),
                    (HAlign::Fill, "Fill"),
                    (HAlign::Justify, "Justify"),
                    (HAlign::CenterAcross, "Center Across Selection"),
                    (HAlign::Distributed, "Distributed"),
                ] {
                    ui.radio_value(&mut st.align.h, h, lang.tr(n));
                }
            });
            ui.label(egui::RichText::new(lang.tr("Vertical")).strong());
            ui.horizontal_wrapped(|ui| {
                for (v, n) in [
                    (VAlign::Top, "Top"),
                    (VAlign::Center, "Center"),
                    (VAlign::Bottom, "Bottom"),
                    (VAlign::Justify, "Justify"),
                    (VAlign::Distributed, "Distributed"),
                ] {
                    ui.radio_value(&mut st.align.v, v, lang.tr(n));
                }
            });
            let mut indent = st.align.indent as f32;
            ui.add(egui::Slider::new(&mut indent, 0.0..=15.0).text(lang.tr("Indent")));
            st.align.indent = indent as u8;
            let mut rot = if st.align.rotation == 255 { 0.0 } else { st.align.rotation as f32 };
            ui.add(egui::Slider::new(&mut rot, -90.0..=90.0).text(lang.tr("Orientation (degrees)")));
            st.align.rotation = rot as i16;
            ui.checkbox(&mut st.align.wrap, lang.tr("Wrap text"));
            ui.checkbox(&mut st.align.shrink, lang.tr("Shrink to fit"));
        }
        "Font" => {
            egui::ComboBox::from_label(lang.tr("Font")).selected_text(st.font.name.clone()).show_ui(ui, |ui| {
                for f in crate::fonts::families() {
                    ui.selectable_value(&mut st.font.name, f.clone(), f.clone());
                }
            });
            ui.add(egui::Slider::new(&mut st.font.size, 6.0..=72.0).text(lang.tr("Size")));
            ui.horizontal(|ui| {
                ui.checkbox(&mut st.font.bold, lang.tr("Bold"));
                ui.checkbox(&mut st.font.italic, lang.tr("Italic"));
                ui.checkbox(&mut st.font.strike, lang.tr("Strikethrough"));
            });
            use gridcraft_engine::model::{Underline, VertAlign};
            ui.horizontal(|ui| {
                for (u, n) in [
                    (Underline::None, "None"),
                    (Underline::Single, "Single"),
                    (Underline::Double, "Double"),
                    (Underline::SingleAccounting, "Single Accounting"),
                    (Underline::DoubleAccounting, "Double Accounting"),
                ] {
                    ui.radio_value(&mut st.font.underline, u, lang.tr(n));
                }
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut st.font.vert, VertAlign::Baseline, lang.tr("Normal"));
                ui.radio_value(&mut st.font.vert, VertAlign::Superscript, lang.tr("Superscript"));
                ui.radio_value(&mut st.font.vert, VertAlign::Subscript, lang.tr("Subscript"));
            });
            ui.label(lang.tr("Color:"));
            let colors = app.session.active().map(|doc| doc.wb.theme.colors).unwrap_or_default();
            if let Some(c) = crate::widgets::color_palette(ui, &colors, lang.tr("Automatic")) {
                st.font.color =
                    if c == "none" { gridcraft_engine::model::Color::Auto } else { gridcraft_engine::model::Color::from_hex(&c).unwrap_or_default() };
            }
        }
        "Border" => {
            use gridcraft_engine::model::{BorderLine, BorderStyle};
            let line = BorderLine { style: BorderStyle::Thin, color: gridcraft_engine::model::Color::Auto };
            ui.horizontal(|ui| {
                if ui.button(lang.tr("None")).clicked() {
                    st.borders = Default::default();
                }
                if ui.button(lang.tr("Outline")).clicked() {
                    st.borders.top = line;
                    st.borders.bottom = line;
                    st.borders.left = line;
                    st.borders.right = line;
                }
            });
            for (n, slot) in [
                ("Top", &mut st.borders.top),
                ("Bottom", &mut st.borders.bottom),
                ("Left", &mut st.borders.left),
                ("Right", &mut st.borders.right),
                ("Diagonal ↘", &mut st.borders.diag_down),
                ("Diagonal ↗", &mut st.borders.diag_up),
            ] {
                let n = lang.tr(n);
                ui.horizontal(|ui| {
                    ui.label(format!("{n}:"));
                    for (s, label) in [
                        (BorderStyle::None, "none"),
                        (BorderStyle::Thin, "thin"),
                        (BorderStyle::Medium, "medium"),
                        (BorderStyle::Thick, "thick"),
                        (BorderStyle::Dashed, "dashed"),
                        (BorderStyle::Dotted, "dotted"),
                        (BorderStyle::Double, "double"),
                    ] {
                        let label = lang.tr(label);
                        let response = ui.selectable_label(slot.style == s, label);
                        response
                            .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::SelectableLabel, ui.is_enabled(), format!("{n} {label}")));
                        if response.clicked() {
                            slot.style = s;
                        }
                    }
                });
            }
            let theme = app.session.active().map(|doc| &doc.wb.theme).cloned().unwrap_or_default();
            crate::border_preview::sample(ui, &st.borders, &theme);
        }
        "Fill" => {
            let colors = app.session.active().map(|doc| doc.wb.theme.colors).unwrap_or_default();
            ui.label(lang.tr("Background Color:"));
            if let Some(c) = crate::widgets::color_palette(ui, &colors, lang.tr("No Color")) {
                st.fill = if c == "none" {
                    Default::default()
                } else {
                    gridcraft_engine::model::Fill::solid(gridcraft_engine::model::Color::from_hex(&c).unwrap_or_default())
                };
            }
        }
        "Protection" => {
            ui.checkbox(&mut st.protection.locked, lang.tr("Locked"));
            ui.checkbox(&mut st.protection.hidden, lang.tr("Hidden"));
            ui.label(
                egui::RichText::new(
                    lang.tr("Locking cells or hiding formulas has no effect until you protect the worksheet (Review › Protect Sheet)."),
                )
                .small(),
            );
        }
        _ => {}
    }
    if let Ok(v) = serde_json::to_value(&st) {
        d.values.insert("style".into(), v);
    }
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm && let Some(style) = d.values.get("style").cloned() {
        app.run_or_alert("home.formatCells", json!({"style": style}));
    }
    if !open {
        app.dialog = None;
        *confirm = true; // closes
    }
}

fn insert_function(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Search:");
        let r = ui.add(egui::TextEdit::singleline(&mut d.search).desired_width(300.0).hint_text("e.g. lookup, average, date"));
        r.request_focus();
    });
    let locale = app.ui.language.formula_locale();
    let list = gridcraft_engine::cmd::formulas::function_list();
    let q = d.search.to_ascii_lowercase();
    let hits: Vec<&Json> = list
        .iter()
        .filter(|f| {
            q.is_empty()
                || f["name"].as_str().is_some_and(|n| locale.function_name(n).to_ascii_lowercase().contains(&q))
                || f["description"].as_str().is_some_and(|n| n.to_ascii_lowercase().contains(&q))
        })
        .take(300)
        .collect();
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        for (i, f) in hits.iter().enumerate() {
            let name = locale.function_name(f["name"].as_str().unwrap_or(""));
            let r = ui.add(
                egui::Button::selectable(i == d.list_index, format!("{name:<18} {}", f["category"].as_str().unwrap_or("")))
                    .min_size(vec2(480.0, 20.0)),
            );
            if r.clicked() {
                d.list_index = i;
            }
            if r.double_clicked() {
                d.list_index = i;
                *confirm = true;
            }
        }
    });
    if let Some(f) = hits.get(d.list_index) {
        ui.separator();
        ui.label(egui::RichText::new(crate::formula_locale::signature(f["name"].as_str().unwrap_or(""), locale).unwrap_or_default()).strong());
        ui.label(crate::formula_locale::description(f["name"].as_str().unwrap_or(""), locale).unwrap_or_default());
    }
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm && let Some(n) = hits.get(d.list_index).and_then(|f| f["name"].as_str()) {
        let n = app.session.active().map(|doc| crate::formula_locale::completion_name(n, locale, &doc.wb, doc.wb.active_sheet)).unwrap_or(n);
        let cur = app.editor.as_ref().map(|e| e.text.clone());
        let text = match cur {
            Some(t) if t.starts_with('=') => format!("{t}{n}("),
            _ => format!("={n}("),
        };
        app.begin_edit(Some(text), false);
    }
    if !open {
        *confirm = true;
    }
}

fn find(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog) {
    ui.horizontal(|ui| {
        for tab in ["Find", "Replace"] {
            if ui.selectable_label(d.tab == tab, tab).clicked() {
                d.tab = tab.into();
            }
        }
    });
    let mut what = d.values.get("what").and_then(Json::as_str).unwrap_or("").to_string();
    let mut with = d.values.get("with").and_then(Json::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.label("Find what:");
        ui.add(egui::TextEdit::singleline(&mut what).desired_width(260.0));
    });
    if d.tab == "Replace" {
        ui.horizontal(|ui| {
            ui.label("Replace with:");
            ui.add(egui::TextEdit::singleline(&mut with).desired_width(244.0));
        });
    }
    let mut case = d.values.get("matchCase").and_then(Json::as_bool).unwrap_or(false);
    let mut whole = d.values.get("wholeCell").and_then(Json::as_bool).unwrap_or(false);
    let mut wb = d.values.get("within").and_then(Json::as_str) == Some("workbook");
    ui.horizontal(|ui| {
        ui.checkbox(&mut case, "Match case");
        ui.checkbox(&mut whole, "Match entire cell contents");
        ui.checkbox(&mut wb, "Within workbook");
    });
    d.values.insert("what".into(), json!(what));
    d.values.insert("with".into(), json!(with));
    d.values.insert("matchCase".into(), json!(case));
    d.values.insert("wholeCell".into(), json!(whole));
    d.values.insert("within".into(), json!(if wb { "workbook" } else { "sheet" }));
    let base = json!({"what": what, "matchCase": case, "wholeCell": whole, "within": if wb { "workbook" } else { "sheet" }});
    ui.horizontal(|ui| {
        if d.tab == "Replace" {
            if ui.button("Replace All").clicked() {
                let mut p = base.clone();
                p["with"] = json!(with);
                p["all"] = json!(true);
                match app.run("edit.replace", p) {
                    Ok(r) => app.toast = Some((format!("All done. We made {} replacements.", r["replaced"]), crate::now_ms())),
                    Err(e) => app.message = Some(("Find and Replace".into(), crate::clean_error(&e))),
                }
            }
            if ui.button("Replace").clicked() {
                let mut p = base.clone();
                p["with"] = json!(with);
                p["all"] = json!(false);
                let _ = app.run("edit.replace", p);
                let _ = app.run("edit.find", base.clone());
            }
        }
        if ui.button("Find All").clicked() {
            let mut p = base.clone();
            p["all"] = json!(true);
            d.result = app.run("edit.find", p).ok();
        }
        if ui.button("Find Next").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
            if let Err(e) = app.run("edit.find", base.clone()) {
                app.message = Some(("Find and Replace".into(), crate::clean_error(&e)));
            }
            app.grid.ensure_visible = true;
        }
    });
    if let Some(r) = &d.result
        && let Some(list) = r["results"].as_array()
    {
        ui.separator();
        ui.label(format!("{} cell(s) found", list.len()));
        egui::ScrollArea::vertical().max_height(180.0).show(ui, |ui| {
            for item in list {
                let label = format!(
                    "{}  {}  {}",
                    item["sheet"].as_str().unwrap_or(""),
                    item["cell"].as_str().unwrap_or(""),
                    item["value"].as_str().unwrap_or("")
                );
                if ui.selectable_label(false, label).clicked() {
                    if let Some(s) = item["sheet"].as_str() {
                        let _ = app.run("sheet.activate", json!({"sheet": s}));
                    }
                    let _ = app.run("selection.set", json!({"cell": item["cell"]}));
                    app.grid.ensure_visible = true;
                }
            }
        });
    }
}

fn go_to(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let names: Vec<String> = app.session.active().map(|doc| doc.wb.names.iter().map(|n| n.name.clone()).collect()).unwrap_or_default();
    for n in names {
        if ui.selectable_label(false, &n).clicked() {
            d.values.insert("reference".into(), json!(n));
        }
    }
    let mut r = d.values.get("reference").and_then(Json::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        ui.label("Reference:");
        ui.text_edit_singleline(&mut r).request_focus();
    });
    d.values.insert("reference".into(), json!(r.clone()));
    if ui.button("Special…").clicked() {
        app.open_dialog("goToSpecial", json!({}));
        return;
    }
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm {
        app.run_or_alert("edit.goTo", json!({"reference": r}));
        app.grid.ensure_visible = true;
    }
    if !open {
        *confirm = true;
    }
}

fn sort(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let mut header = d.values.get("header").and_then(Json::as_bool).unwrap_or(true);
    ui.checkbox(&mut header, "My list has headers");
    d.values.insert("header".into(), json!(header));
    let mut levels: Vec<Json> = d.values.get("levels").and_then(Json::as_array).cloned().unwrap_or_default();
    let mut remove = None;
    for (i, lv) in levels.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(if i == 0 { "Sort by" } else { "Then by" });
            let mut col = lv["column"].as_str().unwrap_or("A").to_string();
            ui.add(egui::TextEdit::singleline(&mut col).desired_width(50.0));
            lv["column"] = json!(col.to_ascii_uppercase());
            let mut by = lv["by"].as_str().unwrap_or("values").to_string();
            egui::ComboBox::from_id_salt(("by", i))
                .selected_text(match by.as_str() {
                    "cellColor" => "Cell Color",
                    "fontColor" => "Font Color",
                    _ => "Cell Values",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut by, "values".into(), "Cell Values");
                    ui.selectable_value(&mut by, "cellColor".into(), "Cell Color");
                    ui.selectable_value(&mut by, "fontColor".into(), "Font Color");
                });
            lv["by"] = json!(by);
            let mut desc = lv["order"].as_str() == Some("desc");
            ui.radio_value(&mut desc, false, "A to Z");
            ui.radio_value(&mut desc, true, "Z to A");
            lv["order"] = json!(if desc { "desc" } else { "asc" });
            if ui.small_button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove
        && levels.len() > 1
    {
        levels.remove(i);
    }
    if ui.button("+ Add Level").clicked() {
        levels.push(json!({"column": "B", "order": "asc"}));
    }
    d.values.insert("levels".into(), Json::Array(levels.clone()));
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm {
        app.run_or_alert("data.sort", json!({"header": header, "keys": levels}));
    }
    if !open {
        *confirm = true;
    }
}

fn name_manager(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let list = app.session.run("formulas.nameManager", json!({})).unwrap_or(Json::Null);
    let names = list.as_array().cloned().unwrap_or_default();
    let selected = names.get(d.list_index).cloned();
    let field = |n: &Json, k: &str| n.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    ui.horizontal(|ui| {
        if ui.button("New…").clicked() {
            app.open_dialog("defineName", json!({}));
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new("Edit…")).clicked()
            && let Some(n) = &selected
        {
            app.open_dialog(
                "defineName",
                json!({
                    "name": field(n, "name"), "scope": field(n, "scope"), "comment": field(n, "comment"),
                    "refersTo": field(n, "refersTo"), "oldName": field(n, "name"), "oldScope": field(n, "scope"),
                }),
            );
        }
        if ui.add_enabled(selected.is_some(), egui::Button::new("Delete")).clicked()
            && let Some(n) = &selected
        {
            app.run_or_alert("formulas.deleteName", json!({"name": field(n, "name"), "scope": field(n, "scope")}));
        }
    });
    ui.add_space(4.0);
    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
        egui::Grid::new("names").striped(true).num_columns(4).show(ui, |ui| {
            ui.strong("Name");
            ui.strong("Value");
            ui.strong("Refers To");
            ui.strong("Scope");
            ui.end_row();
            if names.is_empty() {
                ui.label(egui::RichText::new("No names defined").italics());
                ui.end_row();
            }
            for (i, n) in names.iter().enumerate() {
                if ui.selectable_label(i == d.list_index, field(n, "name")).clicked() {
                    d.list_index = i;
                }
                ui.label(n.get("value").map(|v| v.to_string()).unwrap_or_default().chars().take(30).collect::<String>());
                ui.label(field(n, "refersTo"));
                ui.label(field(n, "scope"));
                ui.end_row();
            }
        });
    });
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        if ui.button(" Close ").clicked() {
            *open = false;
        }
    });
}

/// Every chart type the All Charts tab offers: `(type, subtype, label)` for `insert.chart`.
const ALL_CHARTS: [(&str, &str, &str); 20] = [
    ("column", "", "Clustered Column"),
    ("column", "stacked", "Stacked Column"),
    ("column", "stacked100", "100% Stacked Column"),
    ("bar", "", "Clustered Bar"),
    ("bar", "stacked", "Stacked Bar"),
    ("bar", "stacked100", "100% Stacked Bar"),
    ("line", "", "Line"),
    ("line", "markers", "Line with Markers"),
    ("line", "stacked", "Stacked Line"),
    ("pie", "", "Pie"),
    ("doughnut", "", "Doughnut"),
    ("area", "", "Area"),
    ("area", "stacked", "Stacked Area"),
    ("scatter", "", "Scatter"),
    ("scatter", "lines", "Scatter with Lines"),
    ("radar", "", "Radar"),
    ("histogram", "", "Histogram"),
    ("waterfall", "", "Waterfall"),
    ("funnel", "", "Funnel"),
    ("combo", "", "Combo"),
];

/// The chart types listed on the dialog's current tab.
fn chart_choices(d: &Dialog) -> Vec<(String, String, String)> {
    if d.tab == "All Charts" {
        return ALL_CHARTS.iter().map(|(t, s, l)| (t.to_string(), s.to_string(), l.to_string())).collect();
    }
    let field = |x: &Json, k: &str| x.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    d.result
        .as_ref()
        .and_then(Json::as_array)
        .map(|a| a.iter().map(|x| (field(x, "type"), field(x, "subtype"), field(x, "label"))).collect())
        .unwrap_or_default()
}

/// Selects entry `i` of the current tab: OK inserts it.
fn select_chart(d: &mut Dialog, i: usize) {
    let Some((t, sub, _)) = chart_choices(d).into_iter().nth(i) else { return };
    d.list_index = i;
    d.values.insert("type".into(), json!(t));
    d.values.insert("subtype".into(), json!(sub));
}

fn chart_icon(t: &str) -> Icon {
    match t {
        "column" | "bar" | "histogram" | "waterfall" | "funnel" | "combo" => Icon::ChartBar,
        "line" | "area" | "radar" => Icon::ChartLine,
        "pie" | "doughnut" => Icon::ChartPie,
        "scatter" | "bubble" => Icon::ChartScatter,
        _ => Icon::Chart,
    }
}

fn recommended_charts(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    ui.horizontal(|ui| {
        for tab in ["Recommended Charts", "All Charts"] {
            if ui.selectable_label(d.tab == tab, tab).clicked() && d.tab != tab {
                d.tab = tab.to_string();
                select_chart(d, 0);
            }
        }
    });
    ui.separator();
    let choices = chart_choices(d);
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(190.0);
            egui::ScrollArea::vertical().id_salt("chart_choices").max_height(300.0).show(ui, |ui| {
                for (i, (kind, _, label)) in choices.iter().enumerate() {
                    let resp = ui
                        .horizontal(|ui| {
                            let (r, _) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::hover());
                            icons::paint(ui.painter(), r.shrink(2.0), chart_icon(kind), t.text);
                            ui.selectable_label(i == d.list_index, label.as_str())
                        })
                        .inner;
                    if resp.clicked() {
                        select_chart(d, i);
                    }
                    if resp.double_clicked() {
                        select_chart(d, i);
                        *confirm = true;
                    }
                }
            });
        });
        ui.add_space(12.0);
        ui.vertical(|ui| {
            let label = choices.get(d.list_index).map(|c| c.2.clone()).unwrap_or_default();
            ui.strong(label);
            let (rect, _) = ui.allocate_exact_size(vec2(360.0, 216.0), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, Color32::WHITE);
            ui.painter().rect_stroke(rect, 2.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
            let params = json!({"type": d.values.get("type").cloned().unwrap_or(Json::Null), "subtype": d.values.get("subtype").cloned().unwrap_or(Json::Null)});
            if let Some(doc) = app.session.active()
                && let Ok((sheet, chart)) = gridcraft_engine::cmd::insert::build_chart(&app.session, &params)
            {
                crate::chartview::paint_chart(ui.painter(), rect, &doc.wb, sheet, &chart);
            }
        });
    });
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let params =
            json!({"type": d.values.get("type").cloned().unwrap_or(Json::Null), "subtype": d.values.get("subtype").cloned().unwrap_or(Json::Null)});
        match app.run("insert.chart", params) {
            Ok(r) => {
                // The new chart is selected, as after inserting any object.
                app.selected_chart = r.get("chart").and_then(Json::as_u64).and_then(|id| u32::try_from(id).ok());
                *open = false;
            }
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

fn select_data(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let text = |d: &Dialog, k: &str| d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let edit = |ui: &mut egui::Ui, d: &mut Dialog, key: &str, label: &str| {
        ui.label(label);
        let mut v = text(d, key);
        if ui.add(egui::TextEdit::singleline(&mut v).desired_width(420.0)).changed() {
            d.values.insert(key.into(), json!(v));
        }
    };
    edit(ui, d, "range", "Chart data range:");
    ui.add_space(6.0);
    ui.strong("Legend Entries (Series)");
    let names: Vec<String> =
        d.values.get("names").and_then(Json::as_array).map(|a| a.iter().map(|n| n.as_str().unwrap_or("").to_string()).collect()).unwrap_or_default();
    let mut remove: Vec<u64> =
        d.values.get("remove").and_then(Json::as_array).map(|a| a.iter().filter_map(Json::as_u64).collect()).unwrap_or_default();
    let kept = names.len().saturating_sub(remove.len());
    egui::ScrollArea::vertical().id_salt("series").max_height(160.0).show(ui, |ui| {
        for (i, name) in names.iter().enumerate() {
            let i = i as u64;
            let removed = remove.contains(&i);
            ui.horizontal(|ui| {
                let label = if name.is_empty() { format!("Series{}", i + 1) } else { name.clone() };
                ui.label(if removed { egui::RichText::new(label).strikethrough().weak() } else { egui::RichText::new(label) });
                if removed {
                    if ui.small_button("Keep").clicked() {
                        remove.retain(|x| *x != i);
                    }
                } else if ui.add_enabled(kept > 1, egui::Button::new("Remove").small()).clicked() {
                    remove.push(i);
                }
            });
        }
    });
    d.values.insert("remove".into(), json!(remove));
    ui.add_space(6.0);
    edit(ui, d, "categories", "Horizontal (Category) Axis Labels:");
    ok_cancel(ui, confirm, open);
    if *confirm {
        *confirm = false;
        let mut params = serde_json::Map::new();
        params.insert("chart".into(), d.values.get("chart").cloned().unwrap_or(Json::Null));
        let range_changed = text(d, "range") != text(d, "range0");
        if range_changed {
            params.insert("range".into(), json!(text(d, "range")));
        } else if !remove.is_empty() {
            // A new range rebuilds the series; removals only apply to the current ones.
            params.insert("removeSeries".into(), json!(remove));
        }
        if text(d, "categories") != text(d, "categories0") || (range_changed && !text(d, "categories").is_empty()) {
            params.insert("categories".into(), json!(text(d, "categories")));
        }
        if params.len() == 1 {
            *open = false;
            return;
        }
        match app.run("chart.selectData", Json::Object(params)) {
            Ok(_) => *open = false,
            Err(e) => app.message = Some((d.title.clone(), crate::clean_error(&e))),
        }
    }
}

fn manage_rules(app: &mut SheetApp, ui: &mut egui::Ui) {
    let list = app.session.run("home.manageRules", json!({})).unwrap_or(Json::Null);
    let rules = list["rules"].as_array().cloned().unwrap_or_default();
    if rules.is_empty() {
        ui.label("There are no conditional formatting rules on this sheet.");
    }
    for (i, r) in rules.iter().enumerate() {
        ui.horizontal(|ui| {
            let kind = r["rule"].as_object().and_then(|o| o.keys().next().cloned()).unwrap_or_default();
            ui.label(format!(
                "{}. {kind}  applies to {}",
                i + 1,
                r["ranges"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect::<Vec<_>>().join(",")).unwrap_or_default()
            ));
            if ui.small_button("▲").clicked() {
                app.run_or_alert("home.manageRules", json!({"moveUp": i}));
            }
            if ui.small_button("▼").clicked() {
                app.run_or_alert("home.manageRules", json!({"moveDown": i}));
            }
            if ui.small_button("Delete").clicked() {
                app.run_or_alert("home.manageRules", json!({"delete": i}));
            }
        });
    }
}

fn command_search(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let r = ui.add(egui::TextEdit::singleline(&mut d.search).desired_width(480.0).hint_text("Type a command, e.g. freeze, chart, bold…"));
    r.request_focus();
    let q = d.search.to_ascii_lowercase();
    let cmds: Vec<(String, String, String)> = app
        .session
        .commands()
        .into_iter()
        .filter(|c| c.enabled)
        .filter(|c| q.is_empty() || c.label.to_ascii_lowercase().contains(&q) || c.id.to_ascii_lowercase().contains(&q))
        .map(|c| (c.id.to_string(), c.label.to_string(), c.menu.join(" › ")))
        .take(40)
        .collect();
    if ui.input(|i| i.key_pressed(Key::ArrowDown)) {
        d.list_index = (d.list_index + 1).min(cmds.len().saturating_sub(1));
    }
    if ui.input(|i| i.key_pressed(Key::ArrowUp)) {
        d.list_index = d.list_index.saturating_sub(1);
    }
    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
        for (i, (id, label, menu)) in cmds.iter().enumerate() {
            let r = ui.add(egui::Button::selectable(i == d.list_index, format!("{label}    {menu}")).min_size(vec2(480.0, 20.0)));
            if r.clicked() {
                d.list_index = i;
                *confirm = true;
            }
            r.on_hover_text(id);
        }
    });
    if ui.input(|i| i.key_pressed(Key::Enter)) {
        *confirm = true;
    }
    if *confirm && let Some((id, ..)) = cmds.get(d.list_index) {
        let id = id.clone();
        app.dialog = None;
        app.run_or_alert(&id, json!({}));
    }
}

fn goal_seek(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool) {
    let get = |k: &str| d.values.get(k).and_then(Json::as_str).unwrap_or("").to_string();
    let (mut set, mut to, mut changing) = (get("set"), get("to"), get("changing"));
    egui::Grid::new("gs").show(ui, |ui| {
        ui.label("Set cell:");
        ui.text_edit_singleline(&mut set);
        ui.end_row();
        ui.label("To value:");
        ui.text_edit_singleline(&mut to);
        ui.end_row();
        ui.label("By changing cell:");
        ui.text_edit_singleline(&mut changing);
        ui.end_row();
    });
    d.values.insert("set".into(), json!(set.clone()));
    d.values.insert("to".into(), json!(to.clone()));
    d.values.insert("changing".into(), json!(changing.clone()));
    let mut open = true;
    ok_cancel(ui, confirm, &mut open);
    if *confirm {
        match app.run("data.goalSeek", json!({"set": set, "to": to.parse::<f64>().unwrap_or(0.0), "changing": changing})) {
            Ok(r) => app.toast = Some((format!("Goal Seeking found a solution: {}", r["value"]), crate::now_ms())),
            Err(e) => app.message = Some(("Goal Seek".into(), crate::clean_error(&e))),
        }
    }
    if !open {
        *confirm = true;
    }
}

/// A compact accent button, the dialog counterpart of the ribbon's primary actions.
fn accent_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let t = theme::Tokens::get(ui.ctx());
    ui.add(egui::Button::new(egui::RichText::new(text).color(Color32::WHITE)).fill(t.accent).min_size(vec2(76.0, 24.0)))
}

/// One Function Library entry: the Excel category, the DAX name and a parseable template.
/// Placeholders are filled from the loaded model by [`power_pivot_fill_template`]:
/// `{table}`, `{numcol}` (first numeric column), `{textcol}` (first text column), `{measure}`.
struct LibraryFunction {
    category: &'static str,
    name: &'static str,
    template: &'static str,
}

/// The evaluator's core functions, grouped as Excel's Function Library groups them.
/// Category → function → template. Templates use the placeholder vocabulary above.
const LIBRARY_FUNCTIONS: &[LibraryFunction] = &[
    // Aggregation
    LibraryFunction { category: "Aggregation", name: "SUM", template: "SUM({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "AVERAGE", template: "AVERAGE({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNT", template: "COUNT({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNTA", template: "COUNTA({table}[{textcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNTBLANK", template: "COUNTBLANK({table}[{textcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNTROWS", template: "COUNTROWS({table})" },
    LibraryFunction { category: "Aggregation", name: "DISTINCTCOUNT", template: "DISTINCTCOUNT({table}[{textcol}])" },
    LibraryFunction { category: "Aggregation", name: "MAX", template: "MAX({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "MIN", template: "MIN({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "PRODUCT", template: "PRODUCT({table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "SUMX", template: "SUMX({table}, {table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "AVERAGEX", template: "AVERAGEX({table}, {table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNTX", template: "COUNTX({table}, {table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "COUNTAX", template: "COUNTAX({table}, {table}[{textcol}])" },
    LibraryFunction { category: "Aggregation", name: "MAXX", template: "MAXX({table}, {table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "MINX", template: "MINX({table}, {table}[{numcol}])" },
    LibraryFunction { category: "Aggregation", name: "PRODUCTX", template: "PRODUCTX({table}, {table}[{numcol}])" },
    // Logical
    LibraryFunction { category: "Logical", name: "IF", template: "IF(TRUE(), 1, 0)" },
    LibraryFunction { category: "Logical", name: "IFERROR", template: "IFERROR(1/0, 0)" },
    LibraryFunction { category: "Logical", name: "SWITCH", template: "SWITCH(TRUE(), TRUE(), 1, 0)" },
    LibraryFunction { category: "Logical", name: "COALESCE", template: "COALESCE({table}[{numcol}], 0)" },
    LibraryFunction { category: "Logical", name: "AND", template: "AND(TRUE(), FALSE())" },
    LibraryFunction { category: "Logical", name: "OR", template: "OR(TRUE(), FALSE())" },
    LibraryFunction { category: "Logical", name: "NOT", template: "NOT(FALSE())" },
    LibraryFunction { category: "Logical", name: "TRUE", template: "TRUE()" },
    LibraryFunction { category: "Logical", name: "FALSE", template: "FALSE()" },
    // Filter
    LibraryFunction { category: "Filter", name: "CALCULATE", template: "CALCULATE([{measure}], {table}[{textcol}] = \"\")" },
    LibraryFunction { category: "Filter", name: "FILTER", template: "FILTER({table}, {table}[{numcol}] > 0)" },
    LibraryFunction { category: "Filter", name: "ALL", template: "ALL({table})" },
    LibraryFunction { category: "Filter", name: "ALLEXCEPT", template: "ALLEXCEPT({table}, {table}[{textcol}])" },
    LibraryFunction { category: "Filter", name: "ALLSELECTED", template: "ALLSELECTED({table})" },
    LibraryFunction { category: "Filter", name: "VALUES", template: "VALUES({table}[{textcol}])" },
    LibraryFunction { category: "Filter", name: "DISTINCT", template: "DISTINCT({table}[{textcol}])" },
    LibraryFunction { category: "Filter", name: "SELECTEDVALUE", template: "SELECTEDVALUE({table}[{textcol}])" },
    // Table
    LibraryFunction { category: "Table", name: "ROW", template: "ROW(\"Value\", 1)" },
    LibraryFunction { category: "Table", name: "ADDCOLUMNS", template: "ADDCOLUMNS({table}, \"Value\", {table}[{numcol}])" },
    LibraryFunction { category: "Table", name: "SELECTCOLUMNS", template: "SELECTCOLUMNS({table}, \"Value\", {table}[{numcol}])" },
    LibraryFunction { category: "Table", name: "TOPN", template: "TOPN(10, {table}, {table}[{numcol}], 0)" },
    LibraryFunction { category: "Table", name: "UNION", template: "UNION({table}, {table})" },
    LibraryFunction { category: "Table", name: "INTERSECT", template: "INTERSECT({table}, {table})" },
    LibraryFunction { category: "Table", name: "EXCEPT", template: "EXCEPT({table}, {table})" },
    LibraryFunction { category: "Table", name: "CROSSJOIN", template: "CROSSJOIN({table}, {table})" },
    LibraryFunction { category: "Table", name: "DATATABLE", template: "DATATABLE(\"Value\", INTEGER, {{1}, {2}})" },
    // Date
    LibraryFunction { category: "Date", name: "DATE", template: "DATE(2024, 1, 1)" },
    LibraryFunction { category: "Date", name: "YEAR", template: "YEAR(DATE(2024, 1, 1))" },
    LibraryFunction { category: "Date", name: "MONTH", template: "MONTH(DATE(2024, 1, 1))" },
    LibraryFunction { category: "Date", name: "DAY", template: "DAY(DATE(2024, 1, 1))" },
    LibraryFunction { category: "Date", name: "HOUR", template: "HOUR(TIME(12, 30, 0))" },
    LibraryFunction { category: "Date", name: "MINUTE", template: "MINUTE(TIME(12, 30, 0))" },
    LibraryFunction { category: "Date", name: "SECOND", template: "SECOND(TIME(12, 30, 0))" },
    LibraryFunction { category: "Date", name: "WEEKDAY", template: "WEEKDAY(DATE(2024, 1, 1))" },
    LibraryFunction { category: "Date", name: "EOMONTH", template: "EOMONTH(DATE(2024, 1, 1), 0)" },
    LibraryFunction { category: "Date", name: "EDATE", template: "EDATE(DATE(2024, 1, 1), 1)" },
    LibraryFunction { category: "Date", name: "DATEDIFF", template: "DATEDIFF(DATE(2024, 1, 1), DATE(2024, 12, 31), DAY)" },
    LibraryFunction { category: "Date", name: "TIME", template: "TIME(12, 30, 0)" },
    // Math
    LibraryFunction { category: "Math", name: "ABS", template: "ABS(-1)" },
    LibraryFunction { category: "Math", name: "ROUND", template: "ROUND(1.234, 2)" },
    LibraryFunction { category: "Math", name: "ROUNDUP", template: "ROUNDUP(1.234, 2)" },
    LibraryFunction { category: "Math", name: "ROUNDDOWN", template: "ROUNDDOWN(1.234, 2)" },
    LibraryFunction { category: "Math", name: "INT", template: "INT(1.9)" },
    LibraryFunction { category: "Math", name: "MOD", template: "MOD(5, 2)" },
    LibraryFunction { category: "Math", name: "POWER", template: "POWER(2, 3)" },
    LibraryFunction { category: "Math", name: "SQRT", template: "SQRT(9)" },
    LibraryFunction { category: "Math", name: "EXP", template: "EXP(1)" },
    LibraryFunction { category: "Math", name: "LN", template: "LN(2)" },
    LibraryFunction { category: "Math", name: "LOG", template: "LOG(8, 2)" },
    LibraryFunction { category: "Math", name: "LOG10", template: "LOG10(100)" },
    LibraryFunction { category: "Math", name: "SIGN", template: "SIGN(-1)" },
    LibraryFunction { category: "Math", name: "CEILING", template: "CEILING(1.2, 1)" },
    LibraryFunction { category: "Math", name: "FLOOR", template: "FLOOR(1.8, 1)" },
    LibraryFunction { category: "Math", name: "DIVIDE", template: "DIVIDE(10, 2, 0)" },
    LibraryFunction { category: "Math", name: "PI", template: "PI()" },
    LibraryFunction { category: "Math", name: "DEGREES", template: "DEGREES(PI())" },
    LibraryFunction { category: "Math", name: "RADIANS", template: "RADIANS(180)" },
    LibraryFunction { category: "Math", name: "SIN", template: "SIN(0)" },
    LibraryFunction { category: "Math", name: "COS", template: "COS(0)" },
    LibraryFunction { category: "Math", name: "TAN", template: "TAN(0)" },
    LibraryFunction { category: "Math", name: "FACT", template: "FACT(5)" },
    LibraryFunction { category: "Math", name: "GCD", template: "GCD(12, 18)" },
    LibraryFunction { category: "Math", name: "LCM", template: "LCM(4, 6)" },
    // Text
    LibraryFunction { category: "Text", name: "LEN", template: "LEN({table}[{textcol}])" },
    LibraryFunction { category: "Text", name: "UPPER", template: "UPPER({table}[{textcol}])" },
    LibraryFunction { category: "Text", name: "LOWER", template: "LOWER({table}[{textcol}])" },
    LibraryFunction { category: "Text", name: "TRIM", template: "TRIM({table}[{textcol}])" },
    LibraryFunction { category: "Text", name: "LEFT", template: "LEFT({table}[{textcol}], 1)" },
    LibraryFunction { category: "Text", name: "RIGHT", template: "RIGHT({table}[{textcol}], 1)" },
    LibraryFunction { category: "Text", name: "MID", template: "MID({table}[{textcol}], 1, 1)" },
    LibraryFunction { category: "Text", name: "CONCATENATE", template: "CONCATENATE({table}[{textcol}], \"\")" },
    LibraryFunction { category: "Text", name: "EXACT", template: "EXACT({table}[{textcol}], \"\")" },
    LibraryFunction { category: "Text", name: "REPT", template: "REPT({table}[{textcol}], 2)" },
    LibraryFunction { category: "Text", name: "SUBSTITUTE", template: "SUBSTITUTE({table}[{textcol}], \"\", \"\")" },
    LibraryFunction { category: "Text", name: "VALUE", template: "VALUE(\"1\")" },
    LibraryFunction { category: "Text", name: "FIND", template: "FIND(\"\", {table}[{textcol}], 1)" },
    LibraryFunction { category: "Text", name: "SEARCH", template: "SEARCH(\"\", {table}[{textcol}], 1)" },
    LibraryFunction { category: "Text", name: "UNICHAR", template: "UNICHAR(65)" },
    // Information
    LibraryFunction { category: "Information", name: "ISBLANK", template: "ISBLANK(BLANK())" },
    LibraryFunction { category: "Information", name: "ISNUMBER", template: "ISNUMBER(1)" },
    LibraryFunction { category: "Information", name: "ISTEXT", template: "ISTEXT(\"\")" },
    LibraryFunction { category: "Information", name: "ISNONTEXT", template: "ISNONTEXT(1)" },
    LibraryFunction { category: "Information", name: "ISLOGICAL", template: "ISLOGICAL(TRUE())" },
    LibraryFunction { category: "Information", name: "ISERROR", template: "ISERROR(1/0)" },
    LibraryFunction { category: "Information", name: "ISEVEN", template: "ISEVEN(2)" },
    LibraryFunction { category: "Information", name: "ISODD", template: "ISODD(1)" },
    LibraryFunction { category: "Information", name: "ISINTEGER", template: "ISINTEGER(1)" },
    LibraryFunction { category: "Information", name: "ISCURRENCY", template: "ISCURRENCY(1)" },
    // Other
    LibraryFunction { category: "Other", name: "PATH", template: "PATH(1, 2)" },
    LibraryFunction { category: "Other", name: "PATHLENGTH", template: "PATHLENGTH(PATH(1, 2))" },
    LibraryFunction { category: "Other", name: "PATHCONTAINS", template: "PATHCONTAINS(PATH(1, 2), 1)" },
    LibraryFunction { category: "Other", name: "PATHITEM", template: "PATHITEM(PATH(1, 2), 1)" },
    LibraryFunction { category: "Other", name: "BLANK", template: "BLANK()" },
];

const LIBRARY_CATEGORIES: &[&str] = &["All", "Aggregation", "Logical", "Filter", "Table", "Date", "Math", "Text", "Information", "Other"];

fn power_pivot_count(count: usize, singular: &str, plural: &str) -> String {
    if count == 1 { format!("{count} {singular}") } else { format!("{count} {plural}") }
}

fn power_pivot_type_matches(data_type: &str, keys: &[&str]) -> bool {
    let lower = data_type.to_ascii_lowercase();
    keys.iter().any(|key| lower.contains(key))
}

/// Fills a library template from the first loaded table: its name, a numeric column, a text
/// column and the first measure. Falls back to the `Sales[Amount]`/`Sales[Region]` example.
fn power_pivot_fill_template(model: &Json, template: &str) -> String {
    let tables = model.get("tables").and_then(Json::as_array).cloned().unwrap_or_default();
    let table = tables.first();
    let table_name = table.and_then(|table| table.get("name")).and_then(Json::as_str).unwrap_or("Sales").to_string();
    let columns = table.and_then(|table| table.get("columns")).and_then(Json::as_array).cloned().unwrap_or_default();
    let column_name = |entry: &Json| entry.get("name").and_then(Json::as_str).unwrap_or("").to_string();
    let numeric = columns
        .iter()
        .find(|column| {
            let data_type = column.get("type").and_then(Json::as_str).unwrap_or("");
            power_pivot_type_matches(data_type, &["int", "double", "decimal", "number", "currency", "real", "float"])
        })
        .or_else(|| columns.first())
        .map(column_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Amount".to_string());
    let text = columns
        .iter()
        .find(|column| {
            let data_type = column.get("type").and_then(Json::as_str).unwrap_or("");
            power_pivot_type_matches(data_type, &["string", "text"])
        })
        .or_else(|| columns.first())
        .map(column_name)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Region".to_string());
    let measure = model
        .get("measures")
        .and_then(Json::as_array)
        .and_then(|measures| measures.first())
        .and_then(|measure| measure.get("name"))
        .and_then(Json::as_str)
        .unwrap_or("Total Sales")
        .to_string();
    template.replace("{table}", &table_name).replace("{numcol}", &numeric).replace("{textcol}", &text).replace("{measure}", &measure)
}

/// Inserts a filled template into the formula bar (or the Add Measure dialog's formula field).
fn power_pivot_insert_function(d: &mut Dialog, template: &str) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let formula = power_pivot_fill_template(&model, template);
    if d.values.get("overlay").and_then(Json::as_str) == Some("addMeasure") {
        d.values.insert("newMeasureFormula".into(), json!(formula));
    } else {
        d.values.insert("formula".into(), json!(formula));
    }
    d.values.insert("showFunctions".into(), json!(false));
}

/// The Function Library picker, opened from the ribbon. It floats over the dialog so it does not
/// displace the Excel Data View layout.
fn power_pivot_functions_panel(ctx: &egui::Context, d: &mut Dialog) {
    let t = theme::Tokens::get(ctx);
    let category = d.values.get("fnCategory").and_then(Json::as_str).unwrap_or("All").to_string();
    let mut search = d.values.get("fnSearch").and_then(Json::as_str).unwrap_or("").to_string();
    let mut close = false;
    egui::Window::new("Function Picker")
        .id(egui::Id::new("power_pivot_functions"))
        .collapsible(false)
        .resizable(false)
        .default_width(620.0)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -40.0))
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Insert function").font(theme::ui_bold(13.0)).color(t.text));
                ui.add(egui::TextEdit::singleline(&mut search).hint_text("Search functions").desired_width(240.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Close").clicked() {
                        close = true;
                    }
                });
            });
            if search != d.values.get("fnSearch").and_then(Json::as_str).unwrap_or("") {
                d.values.insert("fnSearch".into(), json!(search));
            }
            ui.horizontal_wrapped(|ui| {
                for candidate in LIBRARY_CATEGORIES {
                    if ui.add(egui::Button::selectable(category == *candidate, egui::RichText::new(*candidate).small())).clicked() {
                        d.values.insert("fnCategory".into(), json!(candidate));
                    }
                }
            });
            ui.separator();
            let needle = search.to_ascii_lowercase();
            let current = d.values.get("fnCategory").and_then(Json::as_str).unwrap_or("All").to_string();
            let model = d.values.get("model").cloned().unwrap_or(Json::Null);
            let mut shown = 0usize;
            egui::ScrollArea::vertical().id_salt("pp_fn_list").max_height(210.0).auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for function in LIBRARY_FUNCTIONS {
                        if current != "All" && function.category != current.as_str() {
                            continue;
                        }
                        if !needle.is_empty() && !function.name.to_ascii_lowercase().contains(&needle) {
                            continue;
                        }
                        shown += 1;
                        let hint = power_pivot_fill_template(&model, function.template);
                        if ui.add(egui::Button::new(function.name).small()).on_hover_text(hint).clicked() {
                            power_pivot_insert_function(d, function.template);
                        }
                    }
                });
                if shown == 0 {
                    ui.label(egui::RichText::new("No functions match.").italics().color(t.text_dim));
                }
            });
        });
    if close {
        d.values.insert("showFunctions".into(), json!(false));
    }
}

/// The Power Pivot window, following Excel's Power Pivot layout: the File/Home/Design/Advanced
/// tab strip, the ribbon, and either the Data View grid or the Diagram View canvas. Model
/// commands run through the `dataModel.*` engine commands, so agents and the control channel see
/// the same model the dialog edits.
fn power_pivot(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    // `ui.dialog.set {field: "tab"}` writes the generic dialog tab; mirror it into this window's
    // own tab value so agents and tests drive the same strip the user clicks.
    if !d.tab.is_empty() && matches!(d.tab.as_str(), "File" | "Home" | "Design" | "Advanced") {
        d.values.insert("tab".into(), json!(d.tab.clone()));
    }
    let t = theme::Tokens::get(ui.ctx());
    power_pivot_header(ui, d, &t);
    ui.add_space(3.0);
    power_pivot_tabs(ui, d, &t);
    let tab = d.values.get("tab").and_then(Json::as_str).unwrap_or("Home").to_string();
    if tab == "File" {
        power_pivot_file_menu(app, ui, d, &t, open);
    } else {
        power_pivot_ribbon(app, ui, d, &t);
        if d.values.get("showFunctions").and_then(Json::as_bool).unwrap_or(false) {
            let ctx = ui.ctx().clone();
            power_pivot_functions_panel(&ctx, d);
        }
        ui.add_space(4.0);
        if d.values.get("view").and_then(Json::as_str).unwrap_or("Data") == "Diagram" {
            power_pivot_diagram(app, ui, d, &t);
        } else {
            power_pivot_data_view(app, ui, d, &t);
        }
        ui.add_space(3.0);
        power_pivot_footer(app, ui, d, &t);
    }
    let overlay = d.values.get("overlay").and_then(Json::as_str).unwrap_or("").to_string();
    if !overlay.is_empty() {
        let ctx = ui.ctx().clone();
        power_pivot_overlay(&ctx, app, d, &overlay);
    }
}

/// The dialog's title strip: a data-model icon and the model's size.
fn power_pivot_header(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
        ui.horizontal(|ui| {
            let (icon, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
            icons::paint(ui.painter(), icon, Icon::Table, t.accent);
            ui.label(egui::RichText::new("Data Model").font(theme::ui_bold(14.0)).color(t.text));
            let model = d.values.get("model").cloned().unwrap_or(Json::Null);
            let loaded = model.get("loaded").and_then(Json::as_bool).unwrap_or(false);
            let tables = model.get("tables").and_then(Json::as_array).map_or(0, Vec::len);
            let measures = model.get("measures").and_then(Json::as_array).map_or(0, Vec::len);
            let summary = if loaded {
                format!("{} · {}", power_pivot_count(tables, "table", "tables"), power_pivot_count(measures, "measure", "measures"))
            } else {
                "not loaded".to_string()
            };
            ui.label(egui::RichText::new(summary).small().color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let view = d.values.get("view").and_then(Json::as_str).unwrap_or("Data");
                let label = if loaded { format!("{view} View") } else { "No model loaded".to_string() };
                ui.label(egui::RichText::new(label).small().color(t.text_disabled));
            });
        });
    });
}

/// Excel's in-window tab strip: File, Home, Design and Advanced.
fn power_pivot_tabs(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let current = d.values.get("tab").and_then(Json::as_str).unwrap_or("Home").to_string();
    let strip = egui::Frame::NONE.fill(t.window).inner_margin(egui::Margin { left: 4, right: 4, top: 0, bottom: 0 }).show(ui, |ui| {
        ui.horizontal(|ui| {
            let file_selected = current == "File";
            let file = ui.add(
                egui::Button::new(egui::RichText::new("File").font(theme::ui_font(12.5)).color(Color32::WHITE))
                    .fill(if file_selected { t.accent_dark } else { t.accent })
                    .min_size(vec2(46.0, 22.0)),
            );
            if file.clicked() {
                d.values.insert("tab".into(), json!("File"));
                d.tab = "File".into();
            }
            for tab in ["Home", "Design", "Advanced"] {
                let selected = current == tab;
                let response = ui.add(egui::Button::selectable(selected, egui::RichText::new(tab).font(theme::ui_font(12.5))));
                if response.clicked() {
                    d.values.insert("tab".into(), json!(tab));
                    d.tab = tab.into();
                }
            }
        });
    });
    let rect = strip.response.rect;
    ui.painter().line_segment([pos2(rect.left(), rect.bottom()), pos2(rect.right(), rect.bottom())], Stroke::new(1.0, t.ribbon_border));
}

/// A ribbon group: its buttons above, Excel's group name underneath.
fn power_pivot_group(ui: &mut egui::Ui, t: &theme::Tokens, label: &str, add: impl FnOnce(&mut egui::Ui)) {
    ui.vertical(|ui| {
        ui.horizontal(|ui| {
            ui.add_space(2.0);
            add(ui);
        });
        ui.add_space(1.0);
        ui.label(egui::RichText::new(label).font(theme::ui_font(10.0)).color(t.text_dim));
    });
}

fn power_pivot_separator(ui: &mut egui::Ui, t: &theme::Tokens) {
    let (rect, _) = ui.allocate_exact_size(vec2(5.0, 84.0), Sense::hover());
    ui.painter().line_segment([pos2(rect.center().x, rect.top() + 6.0), pos2(rect.center().x, rect.bottom() - 6.0)], Stroke::new(1.0, t.separator));
}

/// A big ribbon button with an accessibility label (the painted label is not a widget).
fn power_pivot_button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> egui::Response {
    let response = crate::widgets::big_button(ui, icon, label, tip, false);
    let accessible = label.replace('\n', " ");
    response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &accessible));
    response
}

/// A disabled big ribbon action: Excel draws it greyed, with a tooltip that says why.
fn power_pivot_disabled_big(ui: &mut egui::Ui, label: &str, tip: &str, width: f32) -> egui::Response {
    let response = ui.add_enabled(false, egui::Button::new(egui::RichText::new(label).font(theme::ui_font(11.0))).min_size(vec2(width, 60.0)));
    response.on_disabled_hover_text(tip)
}

/// A disabled small ribbon action (Excel's stacked commands).
fn power_pivot_disabled_small(ui: &mut egui::Ui, label: &str, tip: &str) -> egui::Response {
    let response = ui.add_enabled(false, egui::Button::new(egui::RichText::new(label).small()).min_size(vec2(84.0, 18.0)));
    response.on_disabled_hover_text(tip)
}

/// An enabled small ribbon action.
fn power_pivot_small(ui: &mut egui::Ui, label: &str, tip: &str) -> egui::Response {
    let response = ui.add(egui::Button::new(egui::RichText::new(label).small()).min_size(vec2(84.0, 18.0)));
    response.on_hover_text(tip)
}

/// A disabled formatting icon ($, %, comma, decimals) in a compact button-sized slot.
fn power_pivot_disabled_icon(ui: &mut egui::Ui, icon: Icon, tip: &str) {
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 19.0), Sense::hover());
    let t = theme::Tokens::get(ui.ctx());
    icons::paint(ui.painter(), rect.shrink(3.0), icon, t.text_disabled);
    response.on_hover_text(tip);
}

/// A big button that paints like a toggle when it is on (Excel's View group).
fn power_pivot_toggle(ui: &mut egui::Ui, t: &theme::Tokens, icon: Icon, label: &str, on: bool, tip: &str) -> egui::Response {
    let response = power_pivot_button(ui, icon, label, tip);
    if on {
        ui.painter().rect_stroke(response.rect.shrink(2.0), 4.0, Stroke::new(1.0, t.accent), StrokeKind::Inside);
    }
    response
}

/// A compact stacked toggle, used by View/Calculations so the full Home ribbon fits.
fn power_pivot_small_toggle(ui: &mut egui::Ui, label: &str, on: bool, tip: &str) -> egui::Response {
    let response = ui.add(egui::Button::selectable(on, egui::RichText::new(label).small()).min_size(vec2(96.0, 18.0)));
    response.on_hover_text(tip)
}

/// The tab-specific ribbon, grouped like Excel's Power Pivot window. Excel's commands that this
/// model engine cannot run are present but disabled, as Excel disables them without a selection.
fn power_pivot_ribbon(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let tab = d.values.get("tab").and_then(Json::as_str).unwrap_or("Home").to_string();
    egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        egui::ScrollArea::horizontal().id_salt("pp_ribbon").auto_shrink([false, true]).show(ui, |ui| {
            ui.horizontal(|ui| match tab.as_str() {
                "Design" => power_pivot_ribbon_design(app, ui, d, t),
                "Advanced" => power_pivot_advanced_ribbon(ui, d, t),
                _ => power_pivot_home_ribbon(app, ui, d, t),
            });
        });
    });
}

/// Home: Clipboard, Get External Data, Refresh, PivotTable, Formatting, Sort and Filter, Find,
/// Calculations, View — Excel's order.
fn power_pivot_home_ribbon(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    power_pivot_group(ui, t, "Clipboard", |ui| {
        ui.horizontal(|ui| {
            power_pivot_disabled_big(ui, "Paste", "The model window has no clipboard surface; paste lives in the worksheet.", 54.0);
            ui.vertical(|ui| {
                power_pivot_disabled_small(ui, "Paste Append", "Appending rows is not available in the model window.");
                power_pivot_disabled_small(ui, "Paste Replace", "Replacing model rows is not available.");
                power_pivot_disabled_small(ui, "Copy", "Copying model rows is not available.");
            });
        });
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Get External Data", |ui| {
        power_pivot_disabled_big(ui, "From\nDatabase", "Importing without a model file is not available.", 82.0);
        power_pivot_disabled_big(ui, "From Data\nService", "Data services are not available in GridCraft.", 82.0);
        power_pivot_disabled_big(ui, "From Other\nSources", "Other sources are not available.", 82.0);
        if power_pivot_button(ui, Icon::Link, "Existing\nConnections", "Choose the model file the Data View reads").clicked() {
            d.values.insert("overlay".into(), json!("existingConnections"));
            d.values.insert("error".into(), json!(""));
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Refresh", |ui| {
        if power_pivot_button(ui, Icon::Redo, "Refresh", "Re-read the model from the engine session").clicked() {
            power_pivot_refresh(app, d);
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "PivotTable", |ui| {
        power_pivot_disabled_big(ui, "PivotTable", "Pivot tables live in the worksheet, not inside the Power Pivot window.", 70.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Formatting", |ui| {
        ui.vertical(|ui| {
            let data_type = power_pivot_selected_column_type(d);
            power_pivot_disabled_small(ui, &format!("Data Type : {data_type}"), "Changing a model column's type is not supported.");
            let format = if power_pivot_selected_measure(d).is_some() { "Format : Measure".to_string() } else { format!("Format : {data_type}") };
            power_pivot_disabled_small(ui, &format, "Formatting model columns is not supported.");
            ui.horizontal(|ui| {
                power_pivot_disabled_icon(ui, Icon::Currency, "Currency format");
                power_pivot_disabled_icon(ui, Icon::Percent, "Percent format");
                power_pivot_disabled_icon(ui, Icon::Comma, "Comma format");
                power_pivot_disabled_icon(ui, Icon::DecInc, "Increase Decimal");
                power_pivot_disabled_icon(ui, Icon::DecDec, "Decrease Decimal");
            });
        });
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Sort and Filter", |ui| {
        ui.vertical(|ui| {
            power_pivot_disabled_small(ui, "Sort A to Z", "Sorting model rows is not supported.");
            power_pivot_disabled_small(ui, "Sort Z to A", "Sorting model rows is not supported.");
        });
        ui.vertical(|ui| {
            power_pivot_disabled_small(ui, "Clear Sort", "Clear the model's sort order.");
            power_pivot_disabled_small(ui, "Clear All\nFilters", "Clear the model's filters.");
            power_pivot_disabled_small(ui, "Sort by\nColumn", "Sort by a related column.");
        });
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Find", |ui| {
        power_pivot_disabled_big(ui, "Find", "Find across the model is not available.", 54.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Calculations", |ui| {
        ui.vertical(|ui| {
            power_pivot_disabled_small(ui, "AutoSum", "AutoSum in the model window is not available.");
            power_pivot_disabled_small(ui, "Create KPI", "KPIs are not available for this model.");
        });
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "View", |ui| {
        let view = d.values.get("view").and_then(Json::as_str).unwrap_or("Data").to_string();
        ui.vertical(|ui| {
            if power_pivot_small_toggle(ui, "Data View", view == "Data", "Show the selected table's rows and the Calculation Area").clicked() {
                d.values.insert("view".into(), json!("Data"));
            }
            if power_pivot_small_toggle(ui, "Diagram View", view == "Diagram", "Show table cards and relationships").clicked() {
                d.values.insert("view".into(), json!("Diagram"));
            }
            let hidden = d.values.get("showHidden").and_then(Json::as_bool).unwrap_or(false);
            if power_pivot_small_toggle(ui, "Show Hidden", hidden, "Show hidden model columns").clicked() {
                d.values.insert("showHidden".into(), json!(!hidden));
            }
            let area = d.values.get("calcArea").and_then(Json::as_bool).unwrap_or(true);
            if power_pivot_small_toggle(ui, "Calculation Area", area, "Show the Calculation Area below the grid").clicked() {
                d.values.insert("calcArea".into(), json!(!area));
            }
        });
    });
}

/// Design: Columns, Calculations, Relationships, Table Properties, Mark as Date Table, Date
/// Table and Edit. The Data View / Diagram canvas stays visible below.
fn power_pivot_ribbon_design(_app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    power_pivot_group(ui, t, "Columns", |ui| {
        if power_pivot_button(ui, Icon::Plus, "Add", "Add a measure to the selected table").clicked() {
            power_pivot_open_measure_dialog(d, "Add");
        }
        power_pivot_disabled_big(ui, "Delete", "Deleting a model column is not supported.", 54.0);
        power_pivot_disabled_big(ui, "Freeze", "Freezing columns is a worksheet command.", 54.0);
        power_pivot_disabled_big(ui, "Width", "Column width is a worksheet command.", 54.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Calculations", |ui| {
        if power_pivot_button(ui, Icon::Function, "Insert\nFunction", "Open the Function Library").clicked() {
            let shown = d.values.get("showFunctions").and_then(Json::as_bool).unwrap_or(false);
            d.values.insert("showFunctions".into(), json!(!shown));
        }
        if power_pivot_button(ui, Icon::Calc, "Calculation\nOptions", "Choose Automatic or Manual calculation").clicked() {
            d.values.insert("overlay".into(), json!("calculationOptions"));
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Relationships", |ui| {
        if power_pivot_button(ui, Icon::Link, "Create\nRelationship", "Relate two tables on matching columns").clicked() {
            power_pivot_open_relationship_dialog(d);
        }
        if power_pivot_button(ui, Icon::Link, "Manage\nRelationships", "List, create and delete model relationships").clicked() {
            d.values.insert("overlay".into(), json!("manageRelationships"));
            d.values.insert("relIndex".into(), json!(-1));
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Table Properties", |ui| {
        if power_pivot_button(ui, Icon::Table, "Table\nProperties", "Show the selected table's columns, types and row count").clicked() {
            d.values.insert("overlay".into(), json!("tableProperties"));
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Mark as Date Table", |ui| {
        power_pivot_disabled_big(ui, "Mark as\nDate Table", "Date tables are not supported by this model engine.", 80.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Date Table", |ui| {
        power_pivot_disabled_big(ui, "Date\nTable", "Date table settings are not supported.", 64.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Edit", |ui| {
        power_pivot_disabled_small(ui, "Undo", "Model edits are applied immediately and are not undoable.");
        power_pivot_disabled_small(ui, "Redo", "Model edits are applied immediately and are not undoable.");
    });
}

/// Advanced: Perspectives, Show Implicit Measures, Summarize By and Reporting Properties.
fn power_pivot_advanced_ribbon(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    power_pivot_group(ui, t, "Perspectives", |ui| {
        power_pivot_disabled_big(ui, "Perspectives", "Perspectives are not supported by this model engine.", 84.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Show Implicit Measures", |ui| {
        let shown = d.values.get("showImplicit").and_then(Json::as_bool).unwrap_or(false);
        if power_pivot_toggle(ui, t, Icon::Calc, "Show\nImplicit\nMeasures", shown, "Show measures the model engine derives from columns").clicked() {
            d.values.insert("showImplicit".into(), json!(!shown));
        }
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Summarize By", |ui| {
        power_pivot_disabled_big(ui, "Summarize\nBy", "Changing the default summarization is not supported.", 84.0);
    });
    power_pivot_separator(ui, t);
    power_pivot_group(ui, t, "Reporting Properties", |ui| {
        power_pivot_disabled_big(ui, "Table Detail", "Table detail settings are not supported.", 76.0);
        power_pivot_disabled_big(ui, "Table\nBehavior", "Table behavior settings are not supported.", 76.0);
        power_pivot_disabled_big(ui, "Image URL", "Image URL settings are not supported.", 76.0);
    });
}

/// File tab: Excel's model-window backstage, with the model path moved out of the data canvas.
fn power_pivot_file_menu(_app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens, open: &mut bool) {
    egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).inner_margin(egui::Margin::symmetric(8, 6)).show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                ui.set_width(250.0);
                if ui.add(egui::Button::new("Open Model…").min_size(vec2(230.0, 26.0))).clicked() {
                    d.values.insert("overlay".into(), json!("openModel"));
                    d.values.insert("error".into(), json!(""));
                }
                if ui.add(egui::Button::new("Existing Connections").min_size(vec2(230.0, 26.0))).clicked() {
                    d.values.insert("overlay".into(), json!("existingConnections"));
                    d.values.insert("error".into(), json!(""));
                }
                ui.separator();
                if ui.add(egui::Button::new("Switch to Advanced Mode").min_size(vec2(230.0, 26.0))).clicked() {
                    d.values.insert("tab".into(), json!("Advanced"));
                    d.tab = "Advanced".into();
                }
                ui.separator();
                if ui.add(egui::Button::new("Close").min_size(vec2(230.0, 26.0))).clicked() {
                    *open = false;
                }
                if ui.add(egui::Button::new("Exit").min_size(vec2(230.0, 26.0))).clicked() {
                    *open = false;
                }
            });
            ui.separator();
            ui.vertical(|ui| {
                let model = d.values.get("model").cloned().unwrap_or(Json::Null);
                let loaded = model.get("loaded").and_then(Json::as_bool).unwrap_or(false);
                let tables = model.get("tables").and_then(Json::as_array).map_or(0, Vec::len);
                let measures = model.get("measures").and_then(Json::as_array).map_or(0, Vec::len);
                let relationships = model.get("relationships").and_then(Json::as_array).map_or(0, Vec::len);
                ui.label(egui::RichText::new("Power Pivot Data Model").font(theme::ui_bold(14.0)).color(t.text));
                ui.add_space(2.0);
                let summary = if loaded {
                    format!(
                        "{} · {} · {}",
                        power_pivot_count(tables, "table", "tables"),
                        power_pivot_count(measures, "measure", "measures"),
                        power_pivot_count(relationships, "relationship", "relationships")
                    )
                } else {
                    "No model is loaded.".to_string()
                };
                ui.label(egui::RichText::new(summary).color(t.text_dim));
                ui.add_space(6.0);
                let path = d.values.get("path").and_then(Json::as_str).unwrap_or("");
                let source = if path.trim().is_empty() { "(no model file chosen)".to_string() } else { path.to_string() };
                ui.label(egui::RichText::new("Model file").small().color(t.text_dim));
                ui.monospace(source);
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(
                        "Open Model reads a JSON data model through dataModel.load. The model is session state: measure and relationship edits do not change the workbook.",
                    )
                    .small()
                    .color(t.text_dim),
                );
            });
        });
    });
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// Excel's Data View: the formula bar, the selected table's grid, then the Calculation Area.
fn power_pivot_data_view(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    power_pivot_formula(app, ui, d, t);
    ui.add_space(3.0);
    power_pivot_data_grid(ui, d, t);
    if d.values.get("calcArea").and_then(Json::as_bool).unwrap_or(true) {
        ui.add_space(4.0);
        power_pivot_calculations(app, ui, d, t);
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.add_space(2.0);
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// Excel's Data View grid: a row-number gutter, column headers with filter arrows, the
/// green "Add Column" header, striped rows and both scroll directions. At most 200 rows display.
fn power_pivot_data_grid(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    const MAX_ROWS: usize = 200;
    let data = d.values.get("tableData").cloned().unwrap_or(Json::Null);
    let columns: Vec<String> = data
        .get("columns")
        .and_then(Json::as_array)
        .map(|columns| columns.iter().filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let rows = data.get("rows").and_then(Json::as_array).cloned().unwrap_or_default();
    let selected = d
        .values
        .get("columnIndex")
        .and_then(Json::as_i64)
        .filter(|index| *index >= 0)
        .map(|index| index as usize)
        .filter(|index| *index < columns.len());
    let filter_column = d.values.get("filterColumn").and_then(Json::as_str).unwrap_or("").to_string();
    let filter_value = d.values.get("filterValue").and_then(Json::as_str).unwrap_or("").to_string();
    let filter_index = columns.iter().position(|column| column == &filter_column);
    let visible: Vec<usize> = rows
        .iter()
        .enumerate()
        .filter(|(_, row)| match filter_index {
            Some(column) if !filter_column.is_empty() => row.get(column).map(|cell| json_scalar_text(cell) == filter_value).unwrap_or(false),
            _ => true,
        })
        .map(|(index, _)| index)
        .collect();
    let mut select_column = None;
    let mut choose_filter: Option<(usize, Option<String>)> = None;
    let mut add_column = false;
    egui::Frame::NONE.fill(t.grid_bg).stroke(Stroke::new(1.0, t.ribbon_border)).corner_radius(2.0).inner_margin(0).show(ui, |ui| {
        ui.set_min_height(260.0);
        ui.set_min_width(ui.available_width());
        if columns.is_empty() {
            ui.add_space(10.0);
            ui.label(egui::RichText::new("Load a model and choose a table tab to see its rows.").italics().color(t.text_dim));
            return;
        }
        egui::ScrollArea::both().id_salt("pp_data_grid").max_height(300.0).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("pp_data_grid_rows").striped(true).spacing(vec2(0.0, 0.0)).show(ui, |ui| {
                egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(5, 2)).show(ui, |ui| {
                    ui.add_sized([30.0, 18.0], egui::Label::new(""));
                });
                for (index, name) in columns.iter().enumerate() {
                    let active = selected == Some(index);
                    egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(4, 2)).show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let text =
                                egui::RichText::new(name).font(theme::ui_bold(12.0)).color(if active { t.header_sel_text } else { t.header_text });
                            if ui.add(egui::Button::new(text).frame(false)).clicked() {
                                select_column = Some(index);
                            }
                            let arrow = if filter_column == *name { "▼" } else { "▾" };
                            let mut filter_opened = false;
                            ui.push_id(("pp_filter", index), |ui| {
                                filter_opened = ui
                                    .menu_button(arrow, |ui| {
                                        ui.set_min_width(160.0);
                                        if ui.button("All").clicked() {
                                            choose_filter = Some((index, None));
                                        }
                                        if ui.button("Clear Filter").clicked() {
                                            choose_filter = Some((index, Some(String::new())));
                                        }
                                        ui.separator();
                                        for value in power_pivot_distinct_values(&rows, index).into_iter().take(50) {
                                            let chosen = filter_column == *name && filter_value == value;
                                            if ui.selectable_label(chosen, &value).clicked() {
                                                choose_filter = Some((index, Some(value)));
                                            }
                                        }
                                    })
                                    .response
                                    .clicked();
                            });
                            if filter_opened {
                                select_column = Some(index);
                            }
                        });
                    });
                }
                egui::Frame::NONE.fill(t.accent_soft).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                    ui.set_min_width(200.0);
                    let text = egui::RichText::new("Add Column").italics().color(t.accent);
                    if ui.add(egui::Button::new(text).frame(false)).on_hover_text("Add a measure to this table").clicked() {
                        add_column = true;
                    }
                });
                ui.end_row();
                for row_index in visible.iter().take(MAX_ROWS) {
                    let row = &rows[*row_index];
                    let gutter = egui::RichText::new(format!("{}", row_index + 1)).small().color(t.text_dim);
                    egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(5, 1)).show(ui, |ui| {
                        ui.add_sized([30.0, 18.0], egui::Label::new(gutter));
                    });
                    for cell in row.as_array().into_iter().flatten() {
                        ui.add(egui::Label::new(egui::RichText::new(json_scalar_text(cell)).color(t.cell_text)).selectable(false));
                    }
                    ui.label("");
                    ui.end_row();
                }
            });
            ui.add_space(2.0);
            if visible.len() > MAX_ROWS {
                ui.label(egui::RichText::new(format!("Showing the first {MAX_ROWS} of {} rows.", visible.len())).small().color(t.text_dim));
            }
        });
    });
    if let Some(index) = select_column {
        let name = columns[index].clone();
        d.values.insert("columnIndex".into(), json!(index));
        d.values.insert("column".into(), json!(name));
        d.values.insert("measureIndex".into(), json!(-1));
        d.values.insert("error".into(), json!(""));
    }
    if let Some((index, value)) = choose_filter {
        let name = columns[index].clone();
        match value {
            None => {
                d.values.insert("filterColumn".into(), json!(""));
                d.values.insert("filterValue".into(), json!(""));
            }
            Some(value) if value.is_empty() => {
                d.values.insert("filterColumn".into(), json!(""));
                d.values.insert("filterValue".into(), json!(""));
            }
            Some(value) => {
                d.values.insert("filterColumn".into(), json!(name));
                d.values.insert("filterValue".into(), json!(value));
            }
        }
    }
    if add_column {
        power_pivot_open_measure_dialog(d, "Add");
    }
}

/// Distinct values in one column, for the header filter menu.
fn power_pivot_distinct_values(rows: &[Json], column: usize) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    for row in rows {
        if let Some(cell) = row.get(column) {
            seen.insert(json_scalar_text(cell));
        }
    }
    seen.into_iter().collect()
}

/// Excel's Calculation Area: measures as a Name / Formula / Format grid, with Add, Update and
/// Delete wired to `dataModel.addMeasure`, `dataModel.updateMeasure` and
/// `dataModel.deleteMeasure`.
fn power_pivot_calculations(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let measures = model.get("measures").and_then(Json::as_array).cloned().unwrap_or_default();
    let selected = d.values.get("measureIndex").and_then(Json::as_i64).unwrap_or(-1);
    let mut select = None;
    let mut add = false;
    let mut update = false;
    let mut delete = false;
    egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).corner_radius(3.0).inner_margin(6).show(ui, |ui| {
        ui.set_min_height(72.0);
        ui.set_min_width(ui.available_width());
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Calculation Area").strong().color(t.text));
            ui.label(egui::RichText::new(power_pivot_count(measures.len(), "measure", "measures")).small().color(t.text_dim));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if accent_button(ui, "Add Measure").clicked() {
                    add = true;
                }
                let has_selection = selected >= 0 && (selected as usize) < measures.len();
                if ui.add_enabled(has_selection, egui::Button::new("Update")).clicked() {
                    update = true;
                }
                if ui.add_enabled(has_selection, egui::Button::new("Delete")).clicked() {
                    delete = true;
                }
            });
        });
        ui.add_space(2.0);
        if measures.is_empty() {
            ui.label(egui::RichText::new("No measures in this model. Add Measure starts one.").italics().color(t.text_dim));
            return;
        }
        egui::ScrollArea::vertical().id_salt("pp_calculations").max_height(108.0).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("pp_calculations_grid").striped(true).spacing(vec2(14.0, 3.0)).show(ui, |ui| {
                for heading in ["Name", "Formula", "Format"] {
                    egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(5, 2)).show(ui, |ui| {
                        ui.label(egui::RichText::new(heading).font(theme::ui_bold(12.0)).color(t.header_text));
                    });
                }
                ui.end_row();
                for (index, measure) in measures.iter().enumerate() {
                    let name = measure.get("name").and_then(Json::as_str).unwrap_or("");
                    let formula = measure.get("formula").and_then(Json::as_str).unwrap_or("");
                    let format = measure.get("format").and_then(Json::as_str).unwrap_or("");
                    let chosen = selected == index as i64;
                    if ui.add(egui::Button::selectable(chosen, egui::RichText::new(name))).clicked() {
                        select = Some(index);
                    }
                    ui.label(egui::RichText::new(formula).color(t.text));
                    ui.label(egui::RichText::new(if format.is_empty() { "General" } else { format }).color(t.text_dim));
                    ui.end_row();
                }
            });
        });
    });
    if let Some(index) = select
        && let Some(measure) = measures.get(index)
    {
        let name = measure.get("name").and_then(Json::as_str).unwrap_or("");
        d.values.insert("measureIndex".into(), json!(index));
        d.values.insert("formula".into(), json!(format!("[{name}]")));
        d.values.insert("column".into(), json!(""));
        d.values.insert("columnIndex".into(), json!(-1));
        d.values.insert("resultKind".into(), json!(""));
        d.values.insert("resultText".into(), json!(""));
        d.values.insert("error".into(), json!(""));
    }
    if add {
        power_pivot_open_measure_dialog(d, "Add");
    }
    if update {
        if selected >= 0 && (selected as usize) < measures.len() {
            power_pivot_open_measure_dialog(d, "Update");
        } else {
            d.values.insert("error".into(), json!("Select a measure in the Calculation Area first."));
        }
    }
    if delete && selected >= 0 && (selected as usize) < measures.len() {
        let name = measures[selected as usize].get("name").and_then(Json::as_str).unwrap_or("").to_string();
        match app.session.run("dataModel.deleteMeasure", json!({"name": name})) {
            Ok(status) => {
                d.values.insert("model".into(), status);
                d.values.insert("error".into(), json!(""));
                d.values.insert("formula".into(), json!(""));
                d.values.insert("measureIndex".into(), json!(-1));
                d.values.insert("resultKind".into(), json!(""));
                d.values.insert("resultText".into(), json!(""));
                d.result = None;
            }
            Err(e) => {
                d.values.insert("error".into(), json!(crate::clean_error(&e)));
            }
        }
    }
}

/// The table tabs and the bottom-right Data View / Diagram View control, as Excel has both.
fn power_pivot_footer(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables = model.get("tables").and_then(Json::as_array).cloned().unwrap_or_default();
    let selected = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let view = d.values.get("view").and_then(Json::as_str).unwrap_or("Data").to_string();
    let mut choose = None;
    egui::Frame::NONE.fill(t.tab_bar).stroke(Stroke::new(1.0, t.ribbon_border)).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
        ui.horizontal(|ui| {
            if tables.is_empty() {
                ui.label(egui::RichText::new("No table tabs").small().color(t.text_dim));
            }
            let tabs_width = (ui.available_width() - 200.0).max(100.0);
            ui.allocate_ui_with_layout(vec2(tabs_width, 22.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                egui::ScrollArea::horizontal().id_salt("pp_table_tabs").auto_shrink([false, true]).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (index, table) in tables.iter().enumerate() {
                            let name = table.get("name").and_then(Json::as_str).unwrap_or("Table");
                            if ui.add(egui::Button::selectable(index == selected, egui::RichText::new(name).font(theme::ui_font(12.0)))).clicked() {
                                choose = Some(index);
                            }
                        }
                    });
                });
            });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::selectable(view == "Diagram", egui::RichText::new("Diagram View").small())).clicked() {
                    d.values.insert("view".into(), json!("Diagram"));
                }
                if ui.add(egui::Button::selectable(view == "Data", egui::RichText::new("Data View").small())).clicked() {
                    d.values.insert("view".into(), json!("Data"));
                }
            });
        });
    });
    if let Some(index) = choose {
        power_pivot_select_table(app, d, index);
    }
}

fn power_pivot_load(app: &mut SheetApp, d: &mut Dialog) {
    let path = d.values.get("path").and_then(Json::as_str).unwrap_or("").trim().to_string();
    if path.is_empty() {
        d.values.insert("error".into(), json!("Enter a model JSON path, or paste the model JSON."));
        return;
    }
    // Browser pickers hand back JSON text; native hosts pass a filesystem path.
    let params = if path.starts_with('{') || path.starts_with('[') { json!({"json": path}) } else { json!({"path": path}) };
    match app.session.run("dataModel.load", params) {
        Ok(model) => {
            d.values.insert("model".into(), model);
            d.values.insert("error".into(), json!(""));
            d.values.remove("tableData");
            d.values.insert("tableIndex".into(), json!(0));
            d.values.insert("column".into(), json!(""));
            d.values.insert("columnIndex".into(), json!(0));
            d.values.insert("measureIndex".into(), json!(-1));
            d.values.insert("formula".into(), json!(""));
            d.values.insert("filterColumn".into(), json!(""));
            d.values.insert("filterValue".into(), json!(""));
            d.values.insert("resultKind".into(), json!(""));
            d.values.insert("resultText".into(), json!(""));
            d.result = None;
            power_pivot_select_table(app, d, 0);
        }
        Err(e) => {
            d.values.insert("error".into(), json!(crate::clean_error(&e)));
            d.result = None;
        }
    }
}

fn power_pivot_refresh(app: &mut SheetApp, d: &mut Dialog) {
    match app.session.run("dataModel.status", json!({})) {
        Ok(model) => {
            d.values.insert("model".into(), model.clone());
            d.values.insert("error".into(), json!(""));
            let measures = model.get("measures").and_then(Json::as_array).map_or(0, Vec::len) as i64;
            let selected = d.values.get("measureIndex").and_then(Json::as_i64).unwrap_or(-1);
            if selected >= measures {
                d.values.insert("measureIndex".into(), json!(-1));
                d.values.insert("formula".into(), json!(""));
                d.values.insert("resultKind".into(), json!(""));
                d.values.insert("resultText".into(), json!(""));
            }
            let index = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
            power_pivot_select_table(app, d, index);
        }
        Err(e) => {
            d.values.insert("error".into(), json!(crate::clean_error(&e)));
        }
    }
}

/// Selects a Data View table tab and fetches its rows as a table expression (`Table` + `table`).
fn power_pivot_select_table(app: &mut SheetApp, d: &mut Dialog, index: usize) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables = model.get("tables").and_then(Json::as_array).cloned().unwrap_or_default();
    let Some(table) = tables.get(index) else {
        d.values.remove("tableData");
        return;
    };
    let Some(name) = table.get("name").and_then(Json::as_str) else {
        d.values.remove("tableData");
        return;
    };
    let previous = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let changed = previous != index;
    d.values.insert("tableIndex".into(), json!(index));
    if changed {
        d.values.insert("filterColumn".into(), json!(""));
        d.values.insert("filterValue".into(), json!(""));
        d.values.insert("formula".into(), json!(""));
        d.values.insert("measureIndex".into(), json!(-1));
        d.values.insert("resultKind".into(), json!(""));
        d.values.insert("resultText".into(), json!(""));
        d.result = None;
    }
    let columns: Vec<String> = table
        .get("columns")
        .and_then(Json::as_array)
        .map(|columns| columns.iter().filter_map(|column| column.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let current = d.values.get("column").and_then(Json::as_str).unwrap_or("").to_string();
    let position = if changed { None } else { columns.iter().position(|column| column == &current) };
    let column_index = position.unwrap_or(0).min(columns.len().saturating_sub(1));
    d.values.insert("columnIndex".into(), json!(column_index));
    d.values.insert("column".into(), json!(columns.get(column_index).cloned().unwrap_or_default()));
    match app.session.run("dataModel.evaluate", json!({"formula": name, "table": true})) {
        Ok(data) => {
            d.values.insert("tableData".into(), data);
            d.values.insert("error".into(), json!(""));
        }
        Err(e) => {
            d.values.remove("tableData");
            d.values.insert("error".into(), json!(crate::clean_error(&e)));
        }
    }
}

/// Runs `dataModel.evaluate` for the formula bar. A bare `[Name]` evaluates that measure, which is
/// how the name box and the Calculation Area hand a measure to the bar.
fn power_pivot_evaluate(app: &mut SheetApp, d: &mut Dialog) {
    let raw = d.values.get("formula").and_then(Json::as_str).unwrap_or("").trim().to_string();
    let raw = raw.strip_prefix('=').unwrap_or(&raw).trim().to_string();
    let selected = d.values.get("measureIndex").and_then(Json::as_i64).unwrap_or(-1);
    let selected_name =
        power_pivot_selected_measure(d).and_then(|measure| measure.get("name").and_then(Json::as_str).map(str::to_string)).unwrap_or_default();
    let bare_measure =
        raw.strip_prefix('[').and_then(|inner| inner.strip_suffix(']')).filter(|inner| !inner.is_empty() && !inner.contains(']')).map(str::to_string);
    let (formula, measure) = if !raw.is_empty() {
        (if bare_measure.is_some() { String::new() } else { raw.clone() }, bare_measure.unwrap_or_default())
    } else if selected >= 0 {
        (String::new(), selected_name)
    } else {
        (String::new(), String::new())
    };
    let outcome = if !formula.is_empty() {
        app.session.run("dataModel.evaluate", json!({"formula": formula}))
    } else if !measure.is_empty() {
        app.session.run("dataModel.evaluate", json!({"measure": measure}))
    } else {
        Err("Enter a DAX formula or select a measure.".to_string())
    };
    match outcome {
        Ok(result) => {
            let kind = if formula.is_empty() { "Measure" } else { "Scalar" };
            let text = result.get("value").map(json_scalar_text).unwrap_or_default();
            d.values.insert("error".into(), json!(""));
            d.values.insert("resultKind".into(), json!(kind));
            d.values.insert("resultText".into(), json!(text));
            d.result = Some(result);
        }
        Err(e) => {
            d.values.insert("error".into(), json!(crate::clean_error(&e)));
            d.values.insert("resultKind".into(), json!(""));
            d.values.insert("resultText".into(), json!(""));
            d.result = None;
        }
    }
}

/// The measure selected in the Calculation Area, cloned from the cached status.
fn power_pivot_selected_measure(d: &Dialog) -> Option<Json> {
    let model = d.values.get("model")?;
    let measures = model.get("measures")?.as_array()?;
    let index = d.values.get("measureIndex").and_then(Json::as_i64).unwrap_or(-1);
    if index < 0 {
        return None;
    }
    measures.get(index as usize).cloned()
}

/// The type the selected column reports in `dataModel.status`, for the ribbon's Data Type label.
fn power_pivot_selected_column_type(d: &Dialog) -> String {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let index = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let column = d.values.get("column").and_then(Json::as_str).unwrap_or("");
    model
        .get("tables")
        .and_then(Json::as_array)
        .and_then(|tables| tables.get(index))
        .and_then(|table| table.get("columns"))
        .and_then(Json::as_array)
        .and_then(|columns| columns.iter().find(|entry| entry.get("name").and_then(Json::as_str) == Some(column)))
        .and_then(|entry| entry.get("type"))
        .and_then(Json::as_str)
        .unwrap_or("Text")
        .to_string()
}

/// Opens the Add Measure / Update Measure dialog, prefilled for the selected table or measure.
fn power_pivot_open_measure_dialog(d: &mut Dialog, mode: &str) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables = model.get("tables").and_then(Json::as_array).cloned().unwrap_or_default();
    let table_index = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let table = tables.get(table_index).and_then(|table| table.get("name")).and_then(Json::as_str).unwrap_or("").to_string();
    if mode == "Update" {
        let Some(measure) = power_pivot_selected_measure(d) else {
            d.values.insert("error".into(), json!("Select a measure in the Calculation Area first."));
            return;
        };
        let name = measure.get("name").and_then(Json::as_str).unwrap_or("").to_string();
        let formula = measure.get("formula").and_then(Json::as_str).unwrap_or("").to_string();
        let format = measure.get("format").and_then(Json::as_str).unwrap_or("").to_string();
        let measure_table = measure.get("table").and_then(Json::as_str).unwrap_or("").to_string();
        d.values.insert("newMeasureName".into(), json!(name));
        d.values.insert("newMeasureFormula".into(), json!(formula));
        d.values.insert("newMeasureFormat".into(), json!(format));
        d.values.insert("newMeasureTable".into(), json!(if measure_table.is_empty() { table } else { measure_table }));
    } else {
        d.values.insert("newMeasureName".into(), json!(""));
        d.values.insert("newMeasureFormula".into(), json!(""));
        d.values.insert("newMeasureFormat".into(), json!(""));
        d.values.insert("newMeasureTable".into(), json!(table));
    }
    d.values.insert("measureDialogMode".into(), json!(mode));
    d.values.insert("overlay".into(), json!("addMeasure"));
    d.values.insert("error".into(), json!(""));
}

/// Opens the Create Relationship dialog with the first two tables (or the only table twice).
fn power_pivot_open_relationship_dialog(d: &mut Dialog) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables: Vec<String> = model
        .get("tables")
        .and_then(Json::as_array)
        .map(|tables| tables.iter().filter_map(|table| table.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let from = tables.first().cloned().unwrap_or_default();
    let to = tables.get(1).cloned().unwrap_or_else(|| from.clone());
    let from_column = power_pivot_column_names(&model, &from).into_iter().next().unwrap_or_default();
    let to_column = power_pivot_column_names(&model, &to).into_iter().next().unwrap_or_default();
    d.values.insert("relFromTable".into(), json!(from));
    d.values.insert("relFromColumn".into(), json!(from_column));
    d.values.insert("relToTable".into(), json!(to));
    d.values.insert("relToColumn".into(), json!(to_column));
    d.values.insert("relBidirectional".into(), json!(false));
    d.values.insert("overlay".into(), json!("createRelationship"));
    d.values.insert("error".into(), json!(""));
}

/// Column names of a model table, from `dataModel.status`.
fn power_pivot_column_names(model: &Json, table: &str) -> Vec<String> {
    model
        .get("tables")
        .and_then(Json::as_array)
        .and_then(|tables| tables.iter().find(|entry| entry.get("name").and_then(Json::as_str) == Some(table)))
        .and_then(|entry| entry.get("columns"))
        .and_then(Json::as_array)
        .map(|columns| columns.iter().filter_map(|column| column.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default()
}

/// The formula bar above the data grid: name box, cancel (X), enter (✓), fx, the DAX expression,
/// and the result of the last Evaluate.
fn power_pivot_formula(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let table_index = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let columns: Vec<String> = model
        .get("tables")
        .and_then(Json::as_array)
        .and_then(|tables| tables.get(table_index))
        .and_then(|table| table.get("columns"))
        .and_then(Json::as_array)
        .map(|columns| columns.iter().filter_map(|column| column.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let selected_name = d.values.get("column").and_then(Json::as_str).unwrap_or("").to_string();
    let selected_index = columns.iter().position(|column| column == &selected_name);
    let measure_name =
        power_pivot_selected_measure(d).and_then(|measure| measure.get("name").and_then(Json::as_str).map(str::to_string)).unwrap_or_default();
    let display = if !selected_name.is_empty() {
        format!("[{selected_name}]")
    } else if !measure_name.is_empty() {
        format!("[{measure_name}]")
    } else {
        "[Select column]".to_string()
    };
    let mut formula = d.values.get("formula").and_then(Json::as_str).unwrap_or("").to_string();
    let mut evaluate = false;
    let mut clear = false;
    let mut chosen_column = None;
    let mut functions = false;
    egui::Frame::NONE.fill(t.input_bg).stroke(Stroke::new(1.0, t.input_border)).corner_radius(2.0).inner_margin(egui::Margin::symmetric(4, 2)).show(
        ui,
        |ui| {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("pp_name_box").width(180.0).selected_text(egui::RichText::new(display.clone()).color(t.text)).show_ui(
                    ui,
                    |ui| {
                        if columns.is_empty() {
                            ui.label(egui::RichText::new("No columns").italics().color(t.text_dim));
                        }
                        for (index, name) in columns.iter().enumerate() {
                            let chosen = selected_index == Some(index);
                            if ui.selectable_label(chosen, format!("[{name}]")).clicked() {
                                chosen_column = Some(index);
                            }
                        }
                    },
                );
                if crate::widgets::icon_button(ui, Icon::Close, t.text_dim, "Cancel (X): clear the formula", vec2(22.0, 20.0)).clicked() {
                    clear = true;
                }
                if crate::widgets::icon_button(ui, Icon::Check, t.accent, "Enter (✓): evaluate the formula", vec2(22.0, 20.0)).clicked() {
                    evaluate = true;
                }
                if crate::widgets::icon_button(ui, Icon::Function, t.text_dim, "Insert Function (fx)", vec2(22.0, 20.0)).clicked() {
                    functions = true;
                }
                let width = ui.available_width();
                let response =
                    ui.add_sized([width, 22.0], egui::TextEdit::singleline(&mut formula).frame(egui::Frame::NONE).hint_text("SUM(Sales[Amount])"));
                if response.changed() {
                    d.values.insert("formula".into(), json!(formula));
                }
                if response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                    evaluate = true;
                }
            });
        },
    );
    if clear {
        formula.clear();
        d.values.insert("formula".into(), json!(""));
        d.values.insert("error".into(), json!(""));
        d.values.insert("resultKind".into(), json!(""));
        d.values.insert("resultText".into(), json!(""));
        d.result = None;
    }
    if let Some(index) = chosen_column {
        let name = columns[index].clone();
        d.values.insert("columnIndex".into(), json!(index));
        d.values.insert("column".into(), json!(name));
        d.values.insert("measureIndex".into(), json!(-1));
        d.values.insert("formula".into(), json!(""));
        d.values.insert("resultKind".into(), json!(""));
        d.values.insert("resultText".into(), json!(""));
        d.values.insert("error".into(), json!(""));
        d.result = None;
    }
    if functions {
        let shown = d.values.get("showFunctions").and_then(Json::as_bool).unwrap_or(false);
        d.values.insert("showFunctions".into(), json!(!shown));
    }
    if evaluate {
        power_pivot_evaluate(app, d);
    }
    let kind = d.values.get("resultKind").and_then(Json::as_str).unwrap_or("").to_string();
    let text = d.values.get("resultText").and_then(Json::as_str).unwrap_or("").to_string();
    if !kind.is_empty() {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Result").small().color(t.text_dim));
            ui.label(egui::RichText::new(format!("{kind}: {text}")).font(theme::ui_bold(12.0)).color(t.text));
        });
    }
}

/// Diagram View: Excel's table cards, with relationship lines when the model exposes relationships.
fn power_pivot_diagram(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let loaded = model.get("loaded").and_then(Json::as_bool).unwrap_or(false);
    let tables: Vec<Json> = model.get("tables").and_then(Json::as_array).cloned().unwrap_or_default().into_iter().take(60).collect();
    let relationships = model.get("relationships").and_then(Json::as_array).cloned().unwrap_or_default();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Model Diagram").strong().color(t.text));
        if loaded {
            ui.label(egui::RichText::new(format!("{} tables · {} relationships", tables.len(), relationships.len())).small().color(t.text_dim));
        } else {
            ui.label(egui::RichText::new("No data model loaded.").italics().color(t.text_dim));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if accent_button(ui, "Data View").clicked() {
                d.values.insert("view".into(), json!("Data"));
            }
            if power_pivot_small(ui, "Refresh", "Re-read the model from the engine session").clicked() {
                power_pivot_refresh(app, d);
            }
            if power_pivot_small(ui, "Fit", "Fit the table cards to the window").clicked() {
                d.values.insert("diagramFit".into(), json!(true));
            }
        });
    });
    ui.add_space(2.0);
    if !loaded || tables.is_empty() {
        ui.label(egui::RichText::new("Load a model to see its tables.").small().color(t.text_dim));
        let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
        if !error.is_empty() {
            ui.label(egui::RichText::new(error).color(t.danger).small());
        }
        return;
    }

    let available = (ui.available_width() - 4.0).max(320.0);
    let mut zoom = d.values.get("diagramZoom").and_then(Json::as_f64).unwrap_or(1.0) as f32;
    if d.values.get("diagramFit").and_then(Json::as_bool).unwrap_or(false) {
        let count = tables.len().max(1) as f32;
        let gap = 24.0;
        let base = 210.0;
        zoom = ((available - gap * (count - 1.0)) / (count * base)).clamp(0.55, 1.25);
        d.values.insert("diagramZoom".into(), json!(zoom));
        d.values.insert("diagramFit".into(), json!(false));
    }
    zoom = zoom.clamp(0.55, 1.25);
    const MAX_COLUMNS: usize = 12;
    let card_w = 210.0 * zoom;
    let header_h = 24.0 * zoom;
    let row_h = 18.0 * zoom;
    let pad = 6.0 * zoom;
    let gap = 24.0;
    let max_columns =
        tables.iter().map(|table| table.get("columns").and_then(Json::as_array).map_or(0, Vec::len).min(MAX_COLUMNS)).max().unwrap_or(0);
    let has_more = tables.iter().any(|table| table.get("columns").and_then(Json::as_array).map_or(0, Vec::len) > MAX_COLUMNS);
    let lines = max_columns + usize::from(has_more);
    let card_h = header_h + lines as f32 * row_h + pad * 2.0;
    let per_row = (((available + gap) / (card_w + gap)).floor() as usize).max(1);
    let row_count = tables.len().div_ceil(per_row).max(1);
    let used_cols = tables.len().min(per_row).max(1);
    let used_w = used_cols as f32 * card_w + used_cols.saturating_sub(1) as f32 * gap;
    let total_w = available.max(used_w);
    let total_h = row_count as f32 * card_h + row_count.saturating_sub(1) as f32 * gap;
    egui::ScrollArea::both().id_salt("pp_diagram").max_height(480.0).auto_shrink([false, false]).show(ui, |ui| {
        let (canvas, _) = ui.allocate_exact_size(vec2(total_w, total_h.max(80.0)), Sense::hover());
        let cards: Vec<(String, Rect)> = tables
            .iter()
            .enumerate()
            .map(|(index, table)| {
                let row = index / per_row;
                let column = index % per_row;
                let min = canvas.min + vec2(column as f32 * (card_w + gap), row as f32 * (card_h + gap));
                (table.get("name").and_then(Json::as_str).unwrap_or("Table").to_string(), Rect::from_min_size(min, vec2(card_w, card_h)))
            })
            .collect();
        {
            let painter = ui.painter();
            for relationship in &relationships {
                let from = relationship.get("from_table").and_then(Json::as_str).unwrap_or("");
                let to = relationship.get("to_table").and_then(Json::as_str).unwrap_or("");
                let Some(from_card) = cards.iter().find(|(name, _)| name == from) else { continue };
                let Some(to_card) = cards.iter().find(|(name, _)| name == to) else { continue };
                let start = pos2(from_card.1.right(), from_card.1.center().y);
                let end = pos2(to_card.1.left(), to_card.1.center().y);
                let mid = (start.x + end.x) / 2.0;
                painter.add(egui::Shape::line(vec![start, pos2(mid, start.y), pos2(mid, end.y), end], Stroke::new(1.5, t.accent)));
                painter.circle_filled(start, 2.5, t.accent);
                painter.circle_filled(end, 2.5, t.accent);
            }
            for (index, table) in tables.iter().enumerate() {
                let card = cards[index].1;
                painter.rect_filled(card, 4.0, t.ribbon);
                painter.rect_stroke(card, 4.0, Stroke::new(1.0, t.ribbon_border), StrokeKind::Inside);
                let header = Rect::from_min_size(card.min, vec2(card.width(), header_h));
                painter.rect_filled(header, 4.0, t.header_bg);
                let shown = table.get("columns").and_then(Json::as_array).map_or(0, Vec::len).min(MAX_COLUMNS);
                let extra = table.get("columns").and_then(Json::as_array).map_or(0, Vec::len).saturating_sub(MAX_COLUMNS);
                if extra > 0 {
                    let y = header.bottom() + shown as f32 * row_h;
                    let row_rect = Rect::from_min_size(pos2(card.left() + 6.0, y), vec2(card.width() - 12.0, row_h));
                    painter.with_clip_rect(row_rect).text(
                        row_rect.left_center(),
                        egui::Align2::LEFT_CENTER,
                        format!("+{extra} more columns"),
                        theme::ui_font((10.0 * zoom).max(7.0)),
                        t.text_dim,
                    );
                }
            }
        }
        for (index, table) in tables.iter().enumerate() {
            let card = cards[index].1;
            let header = Rect::from_min_size(card.min, vec2(card.width(), header_h));
            let name = table.get("name").and_then(Json::as_str).unwrap_or("Table");
            ui.scope_builder(
                egui::UiBuilder::new().max_rect(header.shrink2(vec2(6.0, 0.0))).layout(egui::Layout::left_to_right(egui::Align::Center)),
                |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(name).font(theme::ui_bold((12.0 * zoom).max(8.0))).color(t.header_text))
                            .truncate()
                            .selectable(false),
                    );
                },
            );
            for (row, column) in table.get("columns").and_then(Json::as_array).into_iter().flatten().take(MAX_COLUMNS).enumerate() {
                let column = column.get("name").and_then(Json::as_str).unwrap_or("");
                let y = header.bottom() + row as f32 * row_h;
                let row_rect = Rect::from_min_size(pos2(card.left() + 6.0, y), vec2(card.width() - 12.0, row_h));
                ui.scope_builder(egui::UiBuilder::new().max_rect(row_rect).layout(egui::Layout::left_to_right(egui::Align::Center)), |ui| {
                    ui.add(
                        egui::Label::new(egui::RichText::new(column).font(theme::ui_font((11.0 * zoom).max(7.0))).color(t.text))
                            .truncate()
                            .selectable(false),
                    );
                });
            }
        }
    });
    if relationships.is_empty() {
        ui.label(egui::RichText::new("This model exposes no relationships; showing table cards only.").small().color(t.text_dim));
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// One floating dialog for the ribbon commands Excel opens in separate windows.
fn power_pivot_overlay(ctx: &egui::Context, app: &mut SheetApp, d: &mut Dialog, overlay: &str) {
    let t = theme::Tokens::get(ctx);
    let mode = d.values.get("measureDialogMode").and_then(Json::as_str).unwrap_or("Add");
    let title = match overlay {
        "openModel" => "Open Model",
        "existingConnections" => "Existing Connections",
        "manageRelationships" => "Manage Relationships",
        "createRelationship" => "Create Relationship",
        "tableProperties" => "Table Properties",
        "calculationOptions" => "Calculation Options",
        "addMeasure" if mode == "Update" => "Update Measure",
        "addMeasure" => "Add Measure",
        _ => "Power Pivot",
    };
    let mut close = false;
    egui::Window::new(title)
        .id(egui::Id::new("power_pivot_overlay"))
        .collapsible(false)
        .resizable(false)
        .default_width(470.0)
        .anchor(egui::Align2::CENTER_CENTER, vec2(0.0, -30.0))
        .show(ctx, |ui| {
            ui.set_min_width(440.0);
            match overlay {
                "openModel" => power_pivot_open_model_dialog(ui, app, d, &t, &mut close, false),
                "existingConnections" => power_pivot_open_model_dialog(ui, app, d, &t, &mut close, true),
                "manageRelationships" => power_pivot_manage_relationships(ui, app, d, &t, &mut close),
                "createRelationship" => power_pivot_create_relationship(ui, app, d, &t, &mut close),
                "tableProperties" => power_pivot_table_properties(ui, d, &t, &mut close),
                "calculationOptions" => power_pivot_calculation_options(ui, d, &t, &mut close),
                "addMeasure" => power_pivot_measure_dialog(ui, app, d, &t, &mut close),
                _ => close = true,
            }
        });
    if close {
        d.values.insert("overlay".into(), json!(""));
    }
}

/// The Open Model / Existing Connections dialog: the model path now lives here, not in the canvas.
fn power_pivot_open_model_dialog(ui: &mut egui::Ui, app: &mut SheetApp, d: &mut Dialog, t: &theme::Tokens, close: &mut bool, existing: bool) {
    ui.label(egui::RichText::new(if existing { "Connections" } else { "Model file" }).strong().color(t.text));
    if existing {
        let path = d.values.get("path").and_then(Json::as_str).unwrap_or("").to_string();
        let label = if path.trim().is_empty() { "No connection yet".to_string() } else { path };
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
            icons::paint(ui.painter(), rect, Icon::Link, t.accent);
            ui.label(egui::RichText::new(label).color(t.text));
        });
        ui.add_space(4.0);
    }
    let web = app.services.open_async.is_some();
    ui.label(egui::RichText::new(if web { "Model JSON" } else { "Model file (JSON)" }).small().color(t.text_dim));
    let mut path = d.values.get("path").and_then(Json::as_str).unwrap_or("").to_string();
    let width = ui.available_width().max(320.0);
    let changed = if web {
        // The browser has no filesystem path; paste the JSON the picker returned (or its text).
        ui.add_sized([width, 90.0], egui::TextEdit::multiline(&mut path).hint_text("{\"tables\": [...]}")).changed()
    } else {
        ui.add_sized([width, 24.0], egui::TextEdit::singleline(&mut path).hint_text("C:\\models\\model.json")).changed()
    };
    if changed {
        d.values.insert("path".into(), json!(path));
    }
    ui.label(
        egui::RichText::new("The Data View loads this file with dataModel.load. Model edits stay in the session, not the workbook.")
            .small()
            .color(t.text_dim),
    );
    let mut load = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if accent_button(ui, "Load").clicked() {
            load = true;
        }
        if ui.button("Cancel").clicked() {
            *close = true;
        }
    });
    if load {
        power_pivot_load(app, d);
        if d.values.get("error").and_then(Json::as_str).unwrap_or("").is_empty() {
            *close = true;
        }
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// Manage Relationships: the model's list, with Delete and a way into Create Relationship.
fn power_pivot_manage_relationships(ui: &mut egui::Ui, app: &mut SheetApp, d: &mut Dialog, t: &theme::Tokens, close: &mut bool) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let relationships = model.get("relationships").and_then(Json::as_array).cloned().unwrap_or_default();
    let mut selected = d.values.get("relIndex").and_then(Json::as_i64).unwrap_or(-1);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Relationships").strong().color(t.text));
        ui.label(egui::RichText::new(format!("{} defined", relationships.len())).small().color(t.text_dim));
    });
    ui.add_space(3.0);
    if relationships.is_empty() {
        ui.label(egui::RichText::new("No relationships are exposed by this model.").italics().color(t.text_dim));
    } else {
        egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).corner_radius(3.0).inner_margin(4).show(ui, |ui| {
            egui::ScrollArea::vertical().id_salt("pp_manage_relationships").max_height(210.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("pp_manage_relationships_grid").striped(true).spacing(vec2(16.0, 3.0)).show(ui, |ui| {
                    for heading in ["From", "To", "Cross filter"] {
                        egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(5, 2)).show(ui, |ui| {
                            ui.label(egui::RichText::new(heading).font(theme::ui_bold(12.0)).color(t.header_text));
                        });
                    }
                    ui.end_row();
                    for (index, relationship) in relationships.iter().enumerate() {
                        let from_table = relationship.get("from_table").and_then(Json::as_str).unwrap_or("");
                        let from_column = relationship.get("from_column").and_then(Json::as_str).unwrap_or("");
                        let to_table = relationship.get("to_table").and_then(Json::as_str).unwrap_or("");
                        let to_column = relationship.get("to_column").and_then(Json::as_str).unwrap_or("");
                        let chosen = selected == index as i64;
                        if ui.add(egui::Button::selectable(chosen, format!("{from_table}[{from_column}]"))).clicked() {
                            selected = index as i64;
                        }
                        if ui.add(egui::Button::selectable(chosen, format!("{to_table}[{to_column}]"))).clicked() {
                            selected = index as i64;
                        }
                        let bidirectional = relationship.get("bidirectional").and_then(Json::as_bool).unwrap_or(false);
                        ui.label(egui::RichText::new(if bidirectional { "Both" } else { "Single" }).color(t.text_dim));
                        ui.end_row();
                    }
                });
            });
        });
    }
    ui.add_space(4.0);
    let mut delete = false;
    let mut create = false;
    ui.horizontal(|ui| {
        if accent_button(ui, "Create Relationship…").clicked() {
            create = true;
        }
        let valid = selected >= 0 && (selected as usize) < relationships.len();
        if ui.add_enabled(valid, egui::Button::new("Delete")).clicked() {
            delete = true;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button("Close").clicked() {
                *close = true;
            }
        });
    });
    d.values.insert("relIndex".into(), json!(selected));
    if create {
        power_pivot_open_relationship_dialog(d);
        return;
    }
    if delete {
        let relationship = &relationships[selected as usize];
        let params = json!({
            "from_table": relationship.get("from_table").cloned().unwrap_or(Json::Null),
            "from_column": relationship.get("from_column").cloned().unwrap_or(Json::Null),
            "to_table": relationship.get("to_table").cloned().unwrap_or(Json::Null),
            "to_column": relationship.get("to_column").cloned().unwrap_or(Json::Null),
        });
        match app.session.run("dataModel.deleteRelationship", params) {
            Ok(status) => {
                d.values.insert("model".into(), status);
                d.values.insert("error".into(), json!(""));
                d.values.insert("relIndex".into(), json!(-1));
            }
            Err(e) => {
                d.values.insert("error".into(), json!(crate::clean_error(&e)));
            }
        }
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// Create Relationship: From table/column, To table/column, and the bidirectional check box.
fn power_pivot_create_relationship(ui: &mut egui::Ui, app: &mut SheetApp, d: &mut Dialog, t: &theme::Tokens, close: &mut bool) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables: Vec<String> = model
        .get("tables")
        .and_then(Json::as_array)
        .map(|tables| tables.iter().filter_map(|table| table.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    if tables.is_empty() {
        ui.label(egui::RichText::new("Load a model before creating a relationship.").italics().color(t.text_dim));
        if ui.button("Close").clicked() {
            *close = true;
        }
        return;
    }
    let mut from_table = d.values.get("relFromTable").and_then(Json::as_str).unwrap_or("").to_string();
    let mut to_table = d.values.get("relToTable").and_then(Json::as_str).unwrap_or("").to_string();
    if !tables.contains(&from_table) {
        from_table = tables[0].clone();
    }
    if !tables.contains(&to_table) {
        to_table = tables.get(1).cloned().unwrap_or_else(|| tables[0].clone());
    }
    let from_columns = power_pivot_column_names(&model, &from_table);
    let to_columns = power_pivot_column_names(&model, &to_table);
    let mut from_column = d.values.get("relFromColumn").and_then(Json::as_str).unwrap_or("").to_string();
    let mut to_column = d.values.get("relToColumn").and_then(Json::as_str).unwrap_or("").to_string();
    if !from_columns.contains(&from_column) {
        from_column = from_columns.first().cloned().unwrap_or_default();
    }
    if !to_columns.contains(&to_column) {
        to_column = to_columns.first().cloned().unwrap_or_default();
    }
    let mut bidirectional = d.values.get("relBidirectional").and_then(Json::as_bool).unwrap_or(false);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("From").strong().color(t.text));
        ui.label(egui::RichText::new("To").strong().color(t.text));
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("pp_rel_from_table").width(190.0).selected_text(from_table.clone()).show_ui(ui, |ui| {
            for table in &tables {
                ui.selectable_value(&mut from_table, table.clone(), table);
            }
        });
        egui::ComboBox::from_id_salt("pp_rel_to_table").width(190.0).selected_text(to_table.clone()).show_ui(ui, |ui| {
            for table in &tables {
                ui.selectable_value(&mut to_table, table.clone(), table);
            }
        });
    });
    ui.horizontal(|ui| {
        egui::ComboBox::from_id_salt("pp_rel_from_column").width(190.0).selected_text(from_column.clone()).show_ui(ui, |ui| {
            for column in &from_columns {
                ui.selectable_value(&mut from_column, column.clone(), column);
            }
        });
        egui::ComboBox::from_id_salt("pp_rel_to_column").width(190.0).selected_text(to_column.clone()).show_ui(ui, |ui| {
            for column in &to_columns {
                ui.selectable_value(&mut to_column, column.clone(), column);
            }
        });
    });
    ui.checkbox(&mut bidirectional, "Make this relationship bidirectional");
    d.values.insert("relFromTable".into(), json!(from_table));
    d.values.insert("relFromColumn".into(), json!(from_column));
    d.values.insert("relToTable".into(), json!(to_table));
    d.values.insert("relToColumn".into(), json!(to_column));
    d.values.insert("relBidirectional".into(), json!(bidirectional));
    let mut create = false;
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if accent_button(ui, "Create").clicked() {
            create = true;
        }
        if ui.button("Cancel").clicked() {
            *close = true;
        }
    });
    if create {
        let params = json!({
            "from_table": from_table,
            "from_column": from_column,
            "to_table": to_table,
            "to_column": to_column,
            "bidirectional": bidirectional,
        });
        match app.session.run("dataModel.addRelationship", params) {
            Ok(status) => {
                d.values.insert("model".into(), status);
                d.values.insert("error".into(), json!(""));
                d.values.insert("relIndex".into(), json!(-1));
                *close = true;
            }
            Err(e) => {
                d.values.insert("error".into(), json!(crate::clean_error(&e)));
            }
        }
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// Table Properties: read-only name, columns/types and row count for the selected table.
fn power_pivot_table_properties(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens, close: &mut bool) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let index = d.values.get("tableIndex").and_then(Json::as_u64).unwrap_or(0) as usize;
    let table = model.get("tables").and_then(Json::as_array).and_then(|tables| tables.get(index)).cloned();
    let Some(table) = table else {
        ui.label(egui::RichText::new("No table is selected.").italics().color(t.text_dim));
        if ui.button("Close").clicked() {
            *close = true;
        }
        return;
    };
    let name = table.get("name").and_then(Json::as_str).unwrap_or("Table");
    let columns = table.get("columns").and_then(Json::as_array).cloned().unwrap_or_default();
    let rows = table.get("rows").and_then(Json::as_u64).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Table name").small().color(t.text_dim));
        ui.label(egui::RichText::new(name).font(theme::ui_bold(13.0)).color(t.text));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Rows").small().color(t.text_dim));
        ui.label(egui::RichText::new(rows.to_string()).color(t.text));
        ui.label(egui::RichText::new("Columns").small().color(t.text_dim));
        ui.label(egui::RichText::new(columns.len().to_string()).color(t.text));
    });
    ui.add_space(4.0);
    egui::Frame::NONE.fill(t.ribbon).stroke(Stroke::new(1.0, t.ribbon_border)).corner_radius(3.0).inner_margin(4).show(ui, |ui| {
        egui::ScrollArea::vertical().id_salt("pp_table_properties").max_height(220.0).auto_shrink([false, false]).show(ui, |ui| {
            egui::Grid::new("pp_table_properties_grid").striped(true).spacing(vec2(20.0, 3.0)).show(ui, |ui| {
                for heading in ["Column", "Type"] {
                    egui::Frame::NONE.fill(t.header_bg).inner_margin(egui::Margin::symmetric(5, 2)).show(ui, |ui| {
                        ui.label(egui::RichText::new(heading).font(theme::ui_bold(12.0)).color(t.header_text));
                    });
                }
                ui.end_row();
                for column in &columns {
                    let column_name = column.get("name").and_then(Json::as_str).unwrap_or("");
                    let data_type = column.get("type").and_then(Json::as_str).unwrap_or("Text");
                    ui.label(egui::RichText::new(column_name).color(t.text));
                    ui.label(egui::RichText::new(data_type).color(t.text_dim));
                    ui.end_row();
                }
            });
        });
    });
    ui.add_space(4.0);
    if ui.button("Close").clicked() {
        *close = true;
    }
}

/// Calculation Options: Automatic / Manual. Manual is UI state only; edits still commit at once.
fn power_pivot_calculation_options(ui: &mut egui::Ui, d: &mut Dialog, t: &theme::Tokens, close: &mut bool) {
    let mut mode = d.values.get("calcMode").and_then(Json::as_str).unwrap_or("Automatic").to_string();
    ui.label(egui::RichText::new("Calculation").strong().color(t.text));
    ui.radio_value(&mut mode, "Automatic".to_string(), "Automatic");
    ui.radio_value(&mut mode, "Manual".to_string(), "Manual");
    ui.label(
        egui::RichText::new("Manual calculation is a window setting only: measure and relationship commands commit immediately.")
            .small()
            .color(t.text_dim),
    );
    d.values.insert("calcMode".into(), json!(mode));
    ui.add_space(4.0);
    if ui.button("Close").clicked() {
        *close = true;
    }
}

/// Add Measure / Update Measure: Name, Table, Formula and Format.
fn power_pivot_measure_dialog(ui: &mut egui::Ui, app: &mut SheetApp, d: &mut Dialog, t: &theme::Tokens, close: &mut bool) {
    let model = d.values.get("model").cloned().unwrap_or(Json::Null);
    let tables: Vec<String> = model
        .get("tables")
        .and_then(Json::as_array)
        .map(|tables| tables.iter().filter_map(|table| table.get("name")).filter_map(Json::as_str).map(str::to_string).collect())
        .unwrap_or_default();
    let mode = d.values.get("measureDialogMode").and_then(Json::as_str).unwrap_or("Add").to_string();
    let mut name = d.values.get("newMeasureName").and_then(Json::as_str).unwrap_or("").to_string();
    let mut table = d.values.get("newMeasureTable").and_then(Json::as_str).unwrap_or("").to_string();
    if !tables.is_empty() && !tables.contains(&table) {
        table = tables[0].clone();
    }
    let mut formula = d.values.get("newMeasureFormula").and_then(Json::as_str).unwrap_or("").to_string();
    let mut format = d.values.get("newMeasureFormat").and_then(Json::as_str).unwrap_or("").to_string();
    ui.label(egui::RichText::new("Name").small().color(t.text_dim));
    if ui.add_sized([ui.available_width().max(320.0), 23.0], egui::TextEdit::singleline(&mut name).hint_text("Total Sales")).changed() {
        d.values.insert("newMeasureName".into(), json!(name));
    }
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Table").small().color(t.text_dim));
    egui::ComboBox::from_id_salt("pp_new_measure_table").width(220.0).selected_text(if table.is_empty() { "Select…" } else { &table }).show_ui(
        ui,
        |ui| {
            for candidate in &tables {
                ui.selectable_value(&mut table, candidate.clone(), candidate);
            }
        },
    );
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Formula").small().color(t.text_dim));
    let width = ui.available_width().max(320.0);
    egui::Frame::NONE.fill(t.input_bg).stroke(Stroke::new(1.0, t.input_border)).corner_radius(3.0).inner_margin(4).show(ui, |ui| {
        if ui
            .add_sized(
                [width - 10.0, 70.0],
                egui::TextEdit::multiline(&mut formula).frame(egui::Frame::NONE).hint_text("SUM(Sales[Amount])").desired_rows(3),
            )
            .changed()
        {
            d.values.insert("newMeasureFormula".into(), json!(formula));
        }
    });
    ui.add_space(2.0);
    ui.label(egui::RichText::new("Format").small().color(t.text_dim));
    if ui.add_sized([220.0, 23.0], egui::TextEdit::singleline(&mut format).hint_text("Currency")).changed() {
        d.values.insert("newMeasureFormat".into(), json!(format));
    }
    d.values.insert("newMeasureTable".into(), json!(table));
    d.values.insert("newMeasureName".into(), json!(name));
    d.values.insert("newMeasureFormula".into(), json!(formula));
    d.values.insert("newMeasureFormat".into(), json!(format));
    ui.add_space(4.0);
    let mut submit = false;
    let update = mode == "Update";
    ui.horizontal(|ui| {
        if accent_button(ui, if update { "Update" } else { "Add" }).clicked() {
            submit = true;
        }
        if ui.button("Cancel").clicked() {
            *close = true;
        }
    });
    if submit {
        let outcome = if update {
            let selected = power_pivot_selected_measure(d);
            match selected {
                Some(measure) => {
                    let old_name = measure.get("name").and_then(Json::as_str).unwrap_or("").to_string();
                    let mut params = Map::new();
                    params.insert("name".into(), json!(old_name));
                    if !name.trim().is_empty() && name.trim() != old_name {
                        params.insert("new_name".into(), json!(name.trim()));
                    }
                    if !formula.trim().is_empty() {
                        params.insert("formula".into(), json!(formula.trim()));
                    }
                    params.insert("format".into(), if format.trim().is_empty() { Json::Null } else { json!(format.trim()) });
                    app.session.run("dataModel.updateMeasure", Json::Object(params))
                }
                None => Err("Select a measure in the Calculation Area first.".to_string()),
            }
        } else {
            let mut params = Map::new();
            params.insert("name".into(), json!(name.trim()));
            params.insert("formula".into(), json!(formula.trim()));
            if !table.trim().is_empty() {
                params.insert("table".into(), json!(table.trim()));
            }
            if !format.trim().is_empty() {
                params.insert("format".into(), json!(format.trim()));
            }
            app.session.run("dataModel.addMeasure", Json::Object(params))
        };
        match outcome {
            Ok(status) => {
                let chosen = name.trim().to_string();
                let index = status
                    .get("measures")
                    .and_then(Json::as_array)
                    .and_then(|measures| measures.iter().position(|measure| measure.get("name").and_then(Json::as_str) == Some(&chosen)));
                d.values.insert("model".into(), status);
                d.values.insert("error".into(), json!(""));
                d.values.insert("resultKind".into(), json!(""));
                d.values.insert("resultText".into(), json!(""));
                d.values.insert("column".into(), json!(""));
                d.values.insert("columnIndex".into(), json!(-1));
                d.values.insert("measureIndex".into(), json!(index.map_or(-1, |index| index as i64)));
                d.values.insert("formula".into(), json!(if index.is_some() { format!("[{chosen}]") } else { String::new() }));
                d.result = None;
                *close = true;
            }
            Err(e) => {
                d.values.insert("error".into(), json!(crate::clean_error(&e)));
            }
        }
    }
    let error = d.values.get("error").and_then(Json::as_str).unwrap_or("").to_string();
    if !error.is_empty() {
        ui.label(egui::RichText::new(error).color(t.danger).small());
    }
}

/// A model value as the grid shows it: blank for null, Excel-style booleans, no trailing `.0`.
fn json_scalar_text(value: &Json) -> String {
    match value {
        Json::Null => String::new(),
        Json::Bool(b) => if *b { "TRUE" } else { "FALSE" }.to_string(),
        Json::String(s) => s.clone(),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                i.to_string()
            } else if let Some(u) = n.as_u64() {
                u.to_string()
            } else if let Some(f) = n.as_f64() {
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e15 { format!("{f:.0}") } else { f.to_string() }
            } else {
                n.to_string()
            }
        }
        other => other.to_string(),
    }
}

/// The fields of one Analysis ToolPak tool's form (input and options; output is drawn apart).
fn toolpak_fields(tool: &str) -> Vec<Field> {
    use Field::*;
    let grouped = || Choice { key: "groupedBy", label: "Grouped By:", options: vec![("columns", "Columns"), ("rows", "Rows")] };
    let labels = || Check { key: "labels", label: "Labels in first row" };
    let range = || Text { key: "range", label: "Input Range:" };
    let two = || vec![Text { key: "range1", label: "Variable 1 Range:" }, Text { key: "range2", label: "Variable 2 Range:" }];
    match tool {
        "descriptive" => vec![
            range(),
            grouped(),
            labels(),
            Check { key: "summary", label: "Summary statistics" },
            Text { key: "confidence", label: "Confidence Level for Mean (%), blank for none:" },
            Text { key: "kthLargest", label: "Kth Largest (blank for none):" },
            Text { key: "kthSmallest", label: "Kth Smallest (blank for none):" },
        ],
        "histogram" => vec![
            range(),
            Text { key: "bins", label: "Bin Range (blank for automatic bins):" },
            labels(),
            Check { key: "pareto", label: "Pareto (sorted histogram)" },
            Check { key: "cumulative", label: "Cumulative Percentage" },
        ],
        "correlation" | "covariance" | "rankPercentile" => vec![range(), grouped(), labels()],
        "anovaSingle" => vec![range(), grouped(), labels(), Text { key: "alpha", label: "Alpha:" }],
        "regression" => vec![
            Text { key: "inputY", label: "Input Y Range:" },
            Text { key: "inputX", label: "Input X Range:" },
            labels(),
            Check { key: "constantZero", label: "Constant is Zero" },
            Text { key: "confidence", label: "Confidence Level (%):" },
            Check { key: "residuals", label: "Residuals" },
            Check { key: "standardizedResiduals", label: "Standardized Residuals" },
            Check { key: "normalProbability", label: "Normal Probability" },
        ],
        "movingAverage" => {
            vec![range(), labels(), Text { key: "interval", label: "Interval:" }, Check { key: "standardErrors", label: "Standard Errors" }]
        }
        "exponentialSmoothing" => {
            vec![range(), labels(), Text { key: "damping", label: "Damping factor:" }, Check { key: "standardErrors", label: "Standard Errors" }]
        }
        "fTest" => {
            let mut v = two();
            v.extend([labels(), Text { key: "alpha", label: "Alpha:" }]);
            v
        }
        _ => {
            let mut v = two();
            v.extend([
                Text { key: "hypothesizedDifference", label: "Hypothesized Mean Difference:" },
                labels(),
                Text { key: "alpha", label: "Alpha:" },
            ]);
            v
        }
    }
}

/// The starting values of a tool's form: the selection as its input, a new sheet as output.
fn toolpak_defaults(sel: &str, tool: &str) -> Map<String, Json> {
    let mut v = json!({
        "tool": tool, "range": sel, "inputY": "", "inputX": "", "range1": sel, "range2": "", "bins": "",
        "groupedBy": "columns", "labels": false, "summary": true, "confidence": "", "kthLargest": "", "kthSmallest": "",
        "interval": "3", "damping": "0.3", "alpha": "0.05", "hypothesizedDifference": "0",
        "outputTo": "newSheet", "output": "", "newSheet": "",
    });
    if tool == "regression" {
        v["confidence"] = json!("95");
        v["inputY"] = json!(sel);
    }
    v.as_object().cloned().unwrap_or_default()
}

/// Data › Data Analysis: pick a tool, then fill in its form. OK runs `data.analysisToolPak`
/// (through the dialog's command); a report that would replace data asks first.
fn analysis_toolpak(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    use gridcraft_engine::cmd::analysis::TOOLS;
    if d.values.get("askOverwrite").and_then(Json::as_bool) == Some(true) {
        ui.label("The output range already holds data. Replace it with the report?");
        ui.horizontal(|ui| {
            if ui.button("Replace").clicked() {
                d.values.remove("askOverwrite");
                d.values.insert("overwrite".into(), json!(true));
                *confirm = true;
            }
            if ui.button("Cancel").clicked() {
                d.values.remove("askOverwrite");
            }
        });
        return;
    }
    let tool = d.values.get("tool").and_then(Json::as_str).unwrap_or("").to_string();
    let Some((_, label)) = TOOLS.iter().find(|(id, _)| *id == tool) else {
        ui.label(egui::RichText::new("Analysis Tools").strong());
        let pick = d.values.get("pick").and_then(Json::as_str).unwrap_or("").to_string();
        let mut chosen = false;
        egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
            for (id, label) in TOOLS {
                let r = ui.selectable_label(pick == *id, *label);
                if r.clicked() {
                    d.values.insert("pick".into(), json!(id));
                }
                if r.double_clicked() {
                    chosen = true;
                }
            }
        });
        let mut ok = false;
        ok_cancel(ui, &mut ok, open);
        let pick = d.values.get("pick").and_then(Json::as_str).unwrap_or("").to_string();
        if (ok || chosen) && TOOLS.iter().any(|(id, _)| *id == pick) {
            let sel = app.session.active().map(|doc| doc.selection.current().a1()).unwrap_or_default();
            d.values = toolpak_defaults(&sel, &pick);
        }
        return;
    };
    d.title = label.to_string();
    d.fields = toolpak_fields(&tool);
    form_fields(ui, d);
    ui.separator();
    ui.label(egui::RichText::new("Output options").strong());
    let to = d.values.get("outputTo").and_then(Json::as_str).unwrap_or("newSheet").to_string();
    for (key, choice, text_label) in [("output", "range", "Output Range:"), ("newSheet", "newSheet", "New Worksheet Ply:")] {
        ui.horizontal(|ui| {
            if ui.radio(to == choice, text_label).clicked() {
                d.values.insert("outputTo".into(), json!(choice));
            }
            let mut t = d.values.get(key).and_then(Json::as_str).unwrap_or("").to_string();
            if ui.add(egui::TextEdit::singleline(&mut t).desired_width(160.0)).changed() {
                d.values.insert(key.into(), json!(t));
                d.values.insert("outputTo".into(), json!(choice));
            }
        });
    }
    ok_cancel(ui, confirm, open);
}

/// Data › Solver: the Solver Parameters form. Solve runs `data.solver` through the dialog's
/// command; the result opens Solver Results.
fn solver(ui: &mut egui::Ui, d: &mut Dialog, confirm: &mut bool, open: &mut bool) {
    let text = |d: &Dialog, k: &str| d.values.get(k).map(|v| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())).unwrap_or_default();
    let edit = |ui: &mut egui::Ui, d: &mut Dialog, k: &str, width: f32| {
        let mut t = text(d, k);
        if ui.add(egui::TextEdit::singleline(&mut t).desired_width(width)).changed() {
            d.values.insert(k.into(), json!(t));
        }
    };
    ui.horizontal(|ui| {
        ui.label("Set Objective:");
        edit(ui, d, "objective", 200.0);
    });
    let goal = text(d, "goal");
    ui.horizontal(|ui| {
        ui.label("To:");
        for (g, label) in [("max", "Max"), ("min", "Min"), ("value", "Value Of:")] {
            if ui.radio(goal == g, label).clicked() {
                d.values.insert("goal".into(), json!(g));
            }
        }
        edit(ui, d, "value", 80.0);
    });
    ui.label("By Changing Variable Cells:");
    edit(ui, d, "variables", 420.0);
    ui.label("Subject to the Constraints:");
    let list: Vec<String> = d
        .values
        .get("constraints")
        .and_then(Json::as_array)
        .map(|a| a.iter().map(|c| c.as_str().map(str::to_string).unwrap_or_else(|| c.to_string())).collect())
        .unwrap_or_default();
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.set_min_size(vec2(420.0, 90.0));
        egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
            for (i, c) in list.iter().enumerate() {
                if ui.selectable_label(d.list_index == i, c).clicked() {
                    d.list_index = i;
                }
            }
        });
    });
    ui.horizontal(|ui| {
        edit(ui, d, "newLhs", 110.0);
        let op = text(d, "newOp");
        egui::ComboBox::from_id_salt("solverOp").selected_text(op.clone()).width(60.0).show_ui(ui, |ui| {
            for o in ["<=", "=", ">=", "int", "bin"] {
                if ui.selectable_label(op == o, o).clicked() {
                    d.values.insert("newOp".into(), json!(o));
                }
            }
        });
        if !matches!(op.as_str(), "int" | "bin") {
            edit(ui, d, "newRhs", 110.0);
        }
        if ui.button("Add").clicked() {
            let lhs = text(d, "newLhs");
            if !lhs.trim().is_empty() {
                let rhs = if matches!(op.as_str(), "int" | "bin") { String::new() } else { format!(" {}", text(d, "newRhs").trim()) };
                let mut list = list.clone();
                list.push(format!("{} {op}{rhs}", lhs.trim()));
                d.values.insert("constraints".into(), json!(list));
                d.values.insert("newLhs".into(), json!(""));
                d.values.insert("newRhs".into(), json!(""));
            }
        }
        if ui.add_enabled(d.list_index < list.len(), egui::Button::new("Delete")).clicked() {
            let mut list = list.clone();
            if d.list_index < list.len() {
                list.remove(d.list_index);
            }
            d.values.insert("constraints".into(), json!(list));
        }
    });
    let mut nonneg = d.values.get("nonNegative").and_then(Json::as_bool).unwrap_or(true);
    if ui.checkbox(&mut nonneg, "Make Unconstrained Variables Non-Negative").changed() {
        d.values.insert("nonNegative".into(), json!(nonneg));
    }
    let method = text(d, "method");
    ui.horizontal(|ui| {
        ui.label("Select a Solving Method:");
        let label = |m: &str| match m {
            "simplex" => "Simplex LP",
            "evolutionary" => "Evolutionary",
            _ => "GRG Nonlinear",
        };
        egui::ComboBox::from_id_salt("solverMethod").selected_text(label(&method)).show_ui(ui, |ui| {
            for m in ["grg", "simplex", "evolutionary"] {
                if ui.selectable_label(method == m, label(m)).clicked() {
                    d.values.insert("method".into(), json!(m));
                }
            }
        });
    });
    ui.label(
        egui::RichText::new(match method.as_str() {
            "simplex" => "Simplex LP: for linear models; integer and binary constraints by branch and bound.",
            "evolutionary" => "Evolutionary: a genetic search for models that aren't smooth; every variable cell needs bounds.",
            _ => "GRG Nonlinear: a local optimum of a smooth model, from the current values.",
        })
        .small(),
    );
    ui.add_space(8.0);
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        let solve =
            ui.add(egui::Button::new(egui::RichText::new("  Solve  ").color(Color32::WHITE)).fill(crate::theme::Tokens::get(ui.ctx()).accent));
        if solve.clicked() && !text(d, "objective").trim().is_empty() {
            *confirm = true;
        }
        if ui.button(" Close ").clicked() {
            *open = false;
        }
    });
}

/// Solver Results: the outcome, and keeping or restoring the variable cells.
fn solver_results(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let message = d.values.get("message").and_then(Json::as_str).unwrap_or("").to_string();
    ui.label(message);
    let changed = d.values.get("changed").and_then(Json::as_bool).unwrap_or(false);
    let restore = d.values.get("restore").and_then(Json::as_bool).unwrap_or(false);
    if changed {
        ui.add_space(4.0);
        if ui.radio(!restore, "Keep Solver Solution").clicked() {
            d.values.insert("restore".into(), json!(false));
        }
        if ui.radio(restore, "Restore Original Values").clicked() {
            d.values.insert("restore".into(), json!(true));
        }
    }
    // Reports describe the solution whether it is kept or not.
    let solved = d.values.get("values").and_then(Json::as_object).is_some_and(|v| !v.is_empty());
    if solved {
        ui.add_space(4.0);
        ui.label("Reports:");
        for (key, label) in [("answerReport", "Answer"), ("limitsReport", "Limits")] {
            let mut on = d.values.get(key).and_then(Json::as_bool).unwrap_or(false);
            if ui.checkbox(&mut on, label).changed() {
                d.values.insert(key.into(), json!(on));
            }
        }
    }
    let mut confirm = false;
    ok_cancel(ui, &mut confirm, open);
    if confirm {
        // The solution (and the saved model) is one undo step.
        if changed && restore {
            app.run_or_alert("edit.undo", json!({}));
        }
        let reports: Vec<&str> = [("answerReport", "answer"), ("limitsReport", "limits")]
            .into_iter()
            .filter(|(key, _)| d.values.get(*key).and_then(Json::as_bool) == Some(true))
            .map(|(_, kind)| kind)
            .collect();
        if solved && !reports.is_empty() {
            let field = |k: &str| d.values.get(k).cloned().unwrap_or(Json::Null);
            let params = json!({"reports": reports, "model": field("model"), "values": field("values"), "original": field("original"), "message": field("message")});
            app.run_or_alert("data.solverReport", params);
        }
        *open = false;
    }
}

fn spelling(app: &mut SheetApp, ui: &mut egui::Ui, d: &mut Dialog, open: &mut bool) {
    let r = d.result.clone().unwrap_or(Json::Null);
    if let Some(e) = r.get("error").and_then(Json::as_str) {
        ui.label(crate::clean_error(e));
        return;
    }
    let issues = r["issues"].as_array().cloned().unwrap_or_default();
    let Some(it) = issues.get(d.list_index) else {
        ui.label("The spelling check is complete for the entire sheet.");
        if ui.button("OK").clicked() {
            *open = false;
        }
        return;
    };
    let word = it["word"].as_str().unwrap_or("").to_string();
    let cell = it["cell"].as_str().unwrap_or("").to_string();
    ui.label(format!("Not in Dictionary ({cell}):"));
    ui.label(egui::RichText::new(&word).strong().color(Color32::from_rgb(0xC4, 0x2B, 0x1C)));
    let sugg: Vec<String> =
        it["suggestions"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect()).unwrap_or_default();
    ui.label("Suggestions:");
    let mut pick = d.values.get("pick").and_then(Json::as_str).unwrap_or("").to_string();
    for s in &sugg {
        if ui.selectable_label(pick == *s, s).clicked() {
            pick = s.clone();
        }
    }
    if sugg.is_empty() {
        ui.label(egui::RichText::new("(No suggestions)").italics());
    }
    d.values.insert("pick".into(), json!(pick.clone()));
    let _ = app.session.run("selection.set", json!({"cell": cell}));
    ui.horizontal(|ui| {
        if ui.button("Ignore Once").clicked() {
            d.list_index += 1;
        }
        if ui.button("Add to Dictionary").clicked() {
            let _ = app.session.run("review.addToDictionary", json!({"word": word}));
            d.list_index += 1;
        }
        if !pick.is_empty() {
            if ui.button("Change").clicked() {
                app.run_or_alert("review.changeSpelling", json!({"cell": cell, "word": word, "to": pick}));
                d.list_index += 1;
            }
            if ui.button("Change All").clicked() {
                app.run_or_alert("review.changeSpelling", json!({"word": word, "to": pick, "all": true}));
                d.list_index += 1;
            }
        }
    });
}

#[cfg(test)]
mod data_validation_tests {
    use super::*;

    #[test]
    fn list_uses_one_source_and_does_not_submit_a_second_list() {
        let fields = data_validation_fields("list", "between");
        assert!(fields.iter().any(|f| matches!(f, Field::Text { key: "formula1", label: "Source:" })));
        assert!(!fields.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. } | Field::Choice { key: "operator", .. })));

        let dialog = Dialog::form(
            "dataValidation",
            "Data Validation",
            "data.validation",
            fields,
            json!({"type": "list", "operator": "between", "formula1": "ABC,DEF", "formula2": "GHI,JKL", "dropdown": true}),
        );
        let params = build_params(&dialog);
        assert_eq!(params["formula1"], "ABC,DEF");
        assert!(params.get("formula2").is_none());
        assert!(params.get("operator").is_none());
    }

    #[test]
    fn only_between_operators_show_and_submit_two_bounds() {
        let two = data_validation_fields("whole", "between");
        assert!(two.iter().any(|f| matches!(f, Field::Text { key: "formula2", label: "Maximum:" })));
        let one = data_validation_fields("date", "lessOrEqual");
        assert!(one.iter().any(|f| matches!(f, Field::Text { key: "formula1", label: "End date:" })));
        assert!(!one.iter().any(|f| matches!(f, Field::Text { key: "formula2", .. })));

        let dialog = Dialog::form(
            "dataValidation",
            "Data Validation",
            "data.validation",
            one,
            json!({"type": "date", "operator": "lessOrEqual", "formula1": "2026-10-12", "formula2": "2030-01-01"}),
        );
        let params = build_params(&dialog);
        assert_eq!(params["operator"], "lessOrEqual");
        assert_eq!(params["formula1"], "2026-10-12");
        assert!(params.get("formula2").is_none());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_validation_opens_on_the_rule_of_the_active_cell() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        app.run("data.validation", json!({"range": "A1", "type": "whole", "operator": "greater", "formula1": "5"})).unwrap();
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["type"], "whole");
        assert_eq!(d.values["operator"], "greater");
        assert_eq!(d.values["formula1"], "5");
        // OK without edits keeps the rule: 6 is accepted, 4 is rejected.
        app.run(d.command.unwrap(), build_params(&d)).unwrap();
        let doc = app.session.active().unwrap();
        let at = gridcraft_engine::core::CellRef::new(0, 0);
        assert!(gridcraft_engine::cmd::data::check_validation(&doc.wb, 0, at, "6").is_none());
        assert!(gridcraft_engine::cmd::data::check_validation(&doc.wb, 0, at, "4").is_some());
    }

    #[test]
    fn data_validation_without_a_rule_opens_on_a_blank_list() {
        let mut session = gridcraft_engine::Session::default();
        session.new_workbook();
        let mut app = SheetApp::new(session, Default::default());
        let d = Dialog::open(&mut app, "dataValidation", json!({})).unwrap();
        assert_eq!(d.values["type"], "list");
        assert!(d.values.get("formula1").is_none());
    }
}
