//! Draw tab: pen and eraser tools, ink strokes, Ink to Shape, Ink to Math, and the Action Pen
//! (handwriting to text). The handwriting and math models and their recognizers are described in
//! `docs/action-pen.md` and `docs/ink-to-math.md`.

use gridcraft_core::CellRef;
use gridcraft_ink::{Accelerator, InstallState, Manifest, ModelFile, PIX2TEXT_MFR, RecognitionOutput, TROCR_SMALL_HANDWRITTEN};
use gridcraft_model::{Anchor, AnchorMode, Color, Shape, ShapeKind};
use serde_json::{Value as Json, json};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use super::{
    CommandSpec, EngineError, PROTECTED, Result, Session, always, bad, bool_param, cell_param_on, cmd, edit, f64_param, has_doc, ok, str_param,
    u32_param,
};

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(noundo "draw.pen", "Pen", ["Draw", "Tools"], None, "{on?: bool, color?: hex, width?: 2}", has_doc, pen),
        cmd!(noundo "draw.eraser", "Eraser", ["Draw", "Tools"], None, "{on?: bool}", has_doc, eraser),
        cmd!(
            noundo "draw.actionPen",
            "Action Pen",
            ["Draw", "Tools"],
            None,
            "{on?: bool} turns handwriting-to-text on; needs the handwriting model (ink.status)",
            always,
            action_pen
        ),
        cmd!(
            "draw.actionPenInk",
            "Action Pen Ink",
            [],
            None,
            "{points: [[x,y],…], cell?, startedAt?, endedAt?} adds a stroke to the handwriting entry and types the result into the cell",
            always,
            action_pen_ink
        ),
        cmd!(
            query "ink.status",
            "Handwriting Model",
            [],
            None,
            "{} reports whether the handwriting model is installed, where it goes, its licence and size",
            always,
            ink_status
        ),
        cmd!(
            query "ink.mathStatus",
            "Math Model",
            [],
            None,
            "{} reports whether the Ink to Math model is installed, where it goes, its licence and size",
            always,
            math_status
        ),
        cmd!(
            query "ink.modelImport",
            "Import Handwriting Model",
            [],
            None,
            "{path: a folder holding the model files, or one of them} copies and verifies them into the model directory",
            always,
            model_import
        ),
        cmd!(query "ink.modelRemove", "Remove Handwriting Model", [], None, "{} deletes the downloaded model", always, model_remove),
        cmd!(noundo "draw.lasso", "Lasso Select", ["Draw", "Tools"], None, "{} (selects ink by dragging; same as Select Objects)", has_doc, |s, _| {
            if s.draw_tool == "actionPen" {
                s.ink_entry = None;
            }
            s.draw_tool = String::new();
            ok()
        }),
        cmd!("draw.stroke", "Ink Stroke", [], None, "{points: [[x,y],…] in sheet points, color?: hex, width?: 2}", has_doc, stroke),
        cmd!(
            "draw.inkToShape",
            "Ink to Shape",
            ["Draw", "Convert"],
            None,
            "{id?} converts an ink stroke (default: the last) into a rectangle, ellipse, triangle or line",
            has_doc,
            ink_to_shape
        ),
        cmd!(
            "draw.inkToMath",
            "Ink to Math",
            ["Draw", "Convert"],
            None,
            "{id?|ids?: [id], points?: [[x,y],…] or [[[x,y],…],…]} turns the selected ink into an equation (needs the math model, ink.mathStatus)",
            has_doc,
            ink_to_math
        ),
    ]
}

fn pen(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(s.draw_tool != "pen");
    if s.draw_tool == "actionPen" {
        // Another tool replaces the Action Pen: a half-written entry must not leak into it.
        s.ink_entry = None;
    }
    s.draw_tool = if on { "pen".into() } else { String::new() };
    if let Some(c) = str_param(p, "color") {
        s.draw_color = c.to_string();
    }
    if let Some(w) = f64_param(p, "width") {
        s.draw_width = w.clamp(0.5, 40.0) as f32;
    }
    Ok(json!({"tool": s.draw_tool, "color": s.draw_color, "width": s.draw_width}))
}

fn eraser(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(s.draw_tool != "eraser");
    if s.draw_tool == "actionPen" {
        s.ink_entry = None;
    }
    s.draw_tool = if on { "eraser".into() } else { String::new() };
    Ok(json!({"tool": s.draw_tool}))
}

// ---------------------------------------------------------------- models

/// Where `manifest`'s files go, as a person would see it.
fn model_directory(s: &Session, manifest: &Manifest) -> String {
    match s.models_dir.as_deref() {
        Some(root) => manifest.dir(Path::new(root)).display().to_string(),
        None => format!("<models>/{}/ (open Draw › Action Pen in the desktop app)", manifest.id),
    }
}

/// The error `draw.actionPen` and `draw.actionPenInk` give when no recognizer is installed.
fn no_recognizer(s: &Session, cmd: &'static str) -> EngineError {
    bad(
        cmd,
        format!(
            "The handwriting model is not installed, so the Action Pen is unavailable. Download it from Draw › Action Pen, or copy its files into {}.",
            model_directory(s, &TROCR_SMALL_HANDWRITTEN)
        ),
    )
}

/// The error `draw.inkToMath` gives when the math model is not installed: the same structured,
/// actionable shape as [`no_recognizer`], naming the folder to fill.
fn no_math_recognizer(s: &Session, cmd: &'static str) -> EngineError {
    bad(
        cmd,
        format!(
            "The math model is not installed, so Ink to Math is unavailable. Download it from Draw › Convert › Ink to Math, or copy its files into {}.",
            model_directory(s, &PIX2TEXT_MFR)
        ),
    )
}

/// The current instant, or `None` where there is no clock: `Instant::now` traps on wasm, so the
/// web build (which has no Action Pen) reads nothing.
fn now() -> Option<Instant> {
    #[cfg(not(target_arch = "wasm32"))]
    #[allow(clippy::disallowed_methods)] // never on wasm (see above)
    let now = Some(Instant::now());
    #[cfg(target_arch = "wasm32")]
    let now = None;
    now
}

/// How long a handwriting entry stays open: a stroke into the same cell within this gap is part
/// of the same entry, so a letter written in several strokes is recognised as one.
pub const INK_ENTRY_GAP: Duration = Duration::from_millis(1_500);

/// Most strokes one entry keeps. Past it the next stroke starts a fresh entry.
pub const MAX_ENTRY_STROKES: usize = 32;

/// Most points one entry keeps, across all its strokes.
pub const MAX_ENTRY_POINTS: usize = 20_000;

/// A handwriting entry: the strokes written into one cell since the entry started. Consecutive
/// strokes are recognised together (without joining them), so multi-stroke letters work; the
/// entry ends on a pause longer than [`INK_ENTRY_GAP`], a different cell, sheet or document, or
/// the tool being turned off.
#[derive(Clone, Debug)]
pub struct InkEntry {
    /// The workbook the entry belongs to (`DocState::uid`).
    pub doc: u64,
    pub sheet: usize,
    /// The cell the recognised text goes into.
    pub cell: CellRef,
    /// Each stroke's points, in sheet points, in the order they were written.
    pub strokes: Vec<Vec<[f32; 2]>>,
    /// Points across all strokes, so a hostile stream of strokes stays bounded.
    pub points: usize,
    /// When the last stroke landed (`None` where there is no clock, as on the web: the entry then
    /// never ends on a pause — the web app has no Action Pen, since it has no model directory).
    pub last: Option<Instant>,
    /// The caller's timestamp of that stroke's pen-up, in milliseconds on the caller's own
    /// clock (`endedAt`). When both ends of the pause are reported, the pause is measured from
    /// pen-up to pen-down, so recognising the previous stroke does not count against it.
    pub ended_at: Option<f64>,
}

/// `draw.actionPen {on?}`: turns the handwriting tool on or off. Turning it on without a model
/// is refused with the directory to install it in (never a panic, never a silent no-op).
fn action_pen(s: &mut Session, p: &Json) -> Result<Json> {
    let on = bool_param(p, "on").unwrap_or(s.draw_tool != "actionPen");
    if on && !s.ink.as_ref().is_some_and(|r| r.output() == RecognitionOutput::Text) {
        return Err(no_recognizer(s, "draw.actionPen"));
    }
    s.draw_tool = if on { "actionPen".into() } else { String::new() };
    if !on {
        // Leaving the tool ends the entry: the next time it is used starts a fresh one.
        s.ink_entry = None;
    }
    Ok(json!({"tool": s.draw_tool, "ready": s.ink.as_ref().is_some_and(|r| r.output() == RecognitionOutput::Text)}))
}

/// `draw.actionPenInk {points, cell?}`: recognises the strokes of the handwriting entry being
/// written and types the result into the cell under them (or `cell`). One undo step, through the
/// same input path as typing, so "42" is a number and "3/4/2024" a date; no ink shape is kept.
///
/// Consecutive strokes into the same cell are one entry (see [`InkEntry`]), so a digit or a word
/// written in several strokes is not cut in half; the strokes are rasterised side by side, never
/// joined. The caller may report each stroke's pen-down and pen-up times (`startedAt`, `endedAt`,
/// milliseconds on any monotonic clock): the pause that ends an entry is then the time the pen was
/// up, so recognising the previous stroke is not mistaken for a pause. Without them the time
/// between commands is used.
fn action_pen_ink(s: &mut Session, p: &Json) -> Result<Json> {
    let pts = points_param(p);
    if pts.is_empty() {
        return Err(bad("draw.actionPenInk", "a stroke needs at least one point in `points`"));
    }
    let Some(recognizer) = s.ink.clone().filter(|r| r.output() == RecognitionOutput::Text) else {
        return Err(no_recognizer(s, "draw.actionPenInk"));
    };
    let (doc, si) = {
        let d = s.doc()?;
        (d.uid, d.wb.active_sheet())
    };
    // The target: the explicit `cell`, else the cell under the stroke's first point — a cell the
    // stroke actually crosses. An extreme of the bounding box need not be one (a diagonal from
    // A2 to B1 covers neither A1 nor the stroke's own corner).
    let at = match cell_param_on(s, p, "cell", si)? {
        Some(c) => c,
        None => {
            let sh = s.doc()?.wb.sheet(si).ok_or(EngineError::NoDocument)?;
            let first = pts.first().copied().unwrap_or([0.0, 0.0]);
            CellRef::new(sh.row_at(first[1].max(0.0)), sh.col_at(first[0].max(0.0)))
        }
    };
    let pen = if s.draw_width <= 0.0 { 2.0 } else { s.draw_width }.clamp(0.5, 40.0);
    let now = now();
    let total = pts.len();
    // The caller's own stroke times (milliseconds, any monotonic clock). They make the pause the
    // time the pen was up: recognition and drawing the next stroke take real time, and counting
    // that as a pause would split a character in two.
    let started_at = f64_param(p, "startedAt").filter(|v| v.is_finite());
    let ended_at = f64_param(p, "endedAt").filter(|v| v.is_finite());
    let gap_ms = INK_ENTRY_GAP.as_millis() as f64;
    // Continue the entry when it is the same document, sheet and cell and no real pause has
    // happened; anything else starts a fresh one, so unrelated writing never merges.
    let continues = s.ink_entry.as_ref().is_some_and(|e| {
        let recent = match (started_at, e.ended_at) {
            // Both ends must come from one clock, with the pen-up before the next pen-down; a
            // negative gap means the caller reset or rewound its clock, which must not merge two
            // unrelated entries.
            (Some(started), Some(ended)) => (0.0..=gap_ms).contains(&(started - ended)),
            // Without the caller's times, the gap between commands is all there is.
            _ => match (now, e.last) {
                (Some(now), Some(last)) => now.checked_duration_since(last).is_some_and(|gap| gap <= INK_ENTRY_GAP),
                // No clock (the web): the entry only ends on a different cell, sheet or document.
                _ => true,
            },
        };
        e.doc == doc && e.sheet == si && e.cell == at && e.strokes.len() < MAX_ENTRY_STROKES && e.points + total <= MAX_ENTRY_POINTS && recent
    });
    let mut entry = match s.ink_entry.take() {
        Some(entry) if continues => entry,
        _ => InkEntry { doc, sheet: si, cell: at, strokes: Vec::new(), points: 0, last: now, ended_at: None },
    };
    entry.strokes.push(pts.iter().map(|q| [q[0] as f32, q[1] as f32]).collect());
    entry.points += total;
    entry.last = now;
    entry.ended_at = ended_at;
    let image = {
        let strokes: Vec<&[[f32; 2]]> = entry.strokes.iter().map(Vec::as_slice).collect();
        gridcraft_ink::raster::rasterize_strokes(&strokes, pen).map_err(|e| bad("draw.actionPenInk", e.to_string()))
    };
    // The ink has been written whether or not it can be recognised: keep it, so the next stroke
    // completes the character.
    s.ink_entry = Some(entry);
    let image = image?;
    let text = recognizer.recognize(&image).map_err(|e| bad("draw.actionPenInk", e.to_string()))?.trim().to_string();
    if text.is_empty() {
        return Err(bad("draw.actionPenInk", "Nothing was recognized in that stroke. Write larger, or try again."));
    }
    if text.chars().count() > 1_000 {
        return Err(bad("draw.actionPenInk", "The recognized text is too long for one cell."));
    }
    edit(s, |cx| {
        let cell = super::edit::input_to_cell(&text, si, at, &mut cx.wb)?;
        let sh = cx.sheet_mut(si)?;
        match cell {
            Some(c) => sh.set_cell(at, c),
            None => {
                sh.remove_cell(at);
            }
        }
        cx.touch(si, at);
        Ok(Json::Null)
    })?;
    let value = s.doc()?.wb.sheet(si).map(|sh| sh.value(at)).unwrap_or_default();
    Ok(json!({"cell": at.a1(), "text": text, "value": value.display()}))
}

/// The backend a build without a recognizer would use: CPU, the one every platform has.
const CPU_ONLY: &[Accelerator] = &[Accelerator::Cpu];

/// `ink.status {accelerator?}`: what the UI's model panel shows for the handwriting model. Cheap
/// (one `stat` per model file), so the panel may ask every frame.
///
/// `accelerator` asks what a specific backend would give: the shipped build runs on CPU, and
/// asking for one it does not have answers with `acceleratorMessage` instead of failing, so a
/// caller can say "runs on CPU; OpenVINO NPU/GPU not available" and carry on (the feature
/// behaves identically on CPU).
fn ink_status(s: &mut Session, p: &Json) -> Result<Json> {
    let recognizer = s.ink.clone().filter(|r| r.output() == RecognitionOutput::Text);
    model_status(
        s,
        p,
        &TROCR_SMALL_HANDWRITTEN,
        recognizer.as_ref(),
        "ink.status",
        "Action Pen is ready: draw with the Action Pen tool and the writing becomes text.",
    )
}

/// `ink.mathStatus {accelerator?}`: the same answer for the Ink to Math model, so its panel can
/// show name, licence, size, folder, status and the missing files before anything is downloaded.
fn math_status(s: &mut Session, p: &Json) -> Result<Json> {
    let recognizer = s.ink_math.clone().filter(|r| r.output() == RecognitionOutput::Math);
    model_status(
        s,
        p,
        &PIX2TEXT_MFR,
        recognizer.as_ref(),
        "ink.mathStatus",
        "Ink to Math is ready: draw a formula, select its ink and choose Draw › Convert › Ink to Math.",
    )
}

/// One model's status: what the panel shows and what every command's refusal names. `ready` is
/// whether a recognizer of the right kind is installed; `ready_hint` is the line shown then.
fn model_status(
    s: &Session,
    p: &Json,
    m: &'static Manifest,
    recognizer: Option<&Arc<dyn gridcraft_ink::Recognizer>>,
    cmd: &'static str,
    ready_hint: &str,
) -> Result<Json> {
    let (state, directory) = match s.models_dir.as_deref().map(PathBuf::from) {
        Some(root) => (m.check_sizes(&root), m.dir(&root).display().to_string()),
        None => (InstallState::Incomplete(vec!["no models directory is configured".to_string()]), model_directory(s, m)),
    };
    let missing = match &state {
        InstallState::Incomplete(files) => files.clone(),
        _ => Vec::new(),
    };
    let files_present = state.is_installed();
    let ready = recognizer.is_some();
    let active = recognizer.map_or(Accelerator::Cpu, |r| r.accelerator());
    let available: &'static [Accelerator] = recognizer.map_or(CPU_ONLY, |r| r.accelerators());
    let requested = str_param(p, "accelerator").map(str::trim).filter(|name| !name.is_empty());
    let mut accelerator_message = None;
    if let Some(name) = requested {
        match Accelerator::parse(name) {
            Some(want) if available.contains(&want) => {}
            Some(want) => {
                accelerator_message = Some(format!(
                    "Ink recognition runs on {}; {} is not available in this build, so it stays on {}.",
                    active.label(),
                    want.label(),
                    active.label()
                ))
            }
            None => {
                return Err(bad(
                    cmd,
                    format!(
                        "unknown accelerator `{name}`; this build runs on {} (available: {})",
                        active.label(),
                        available.iter().map(|a| a.as_str()).collect::<Vec<_>>().join(", ")
                    ),
                ));
            }
        }
    }
    let hint = if ready {
        ready_hint.to_string()
    } else if files_present {
        "The model files are present but were not loaded; restart GridCraft or download the model again.".to_string()
    } else {
        format!(
            "Download the {} model, or copy its files into {directory}.",
            if m.output == RecognitionOutput::Math { "math" } else { "handwriting" }
        )
    };
    Ok(json!({
        "model": m.id,
        "name": m.name,
        "revision": m.revision,
        "licence": m.licence,
        "source": m.source,
        "output": m.output.as_str(),
        "bytes": m.total_bytes(),
        "runtime": m.runtime,
        "accelerator": active.as_str(),
        "accelerators": available.iter().map(|a| a.as_str()).collect::<Vec<_>>(),
        "requestedAccelerator": requested,
        "acceleratorMessage": accelerator_message,
        "directory": directory,
        "filesPresent": files_present,
        "ready": ready,
        "missing": missing,
        "hint": hint,
    }))
}

/// The model a model command names: `"math"` (also `"inkToMath"`, `"equation"`) for Ink to Math,
/// anything else absent or `"text"` for the Action Pen's handwriting model. An unknown name is
/// refused rather than silently importing into the wrong folder.
fn manifest_param(p: &Json, cmd: &'static str) -> Result<&'static Manifest> {
    match str_param(p, "model").map(|m| m.trim().to_ascii_lowercase().replace(['-', '_', ' '], "")) {
        None => Ok(&TROCR_SMALL_HANDWRITTEN),
        Some(m) if m.is_empty() || m == "text" || m == "handwriting" || m == "actionpen" || m == "ink" => Ok(&TROCR_SMALL_HANDWRITTEN),
        Some(m) if m == "math" || m == "inktomath" || m == "equation" => Ok(&PIX2TEXT_MFR),
        Some(other) => Err(bad(cmd, format!("unknown model `{other}`; use \"handwriting\" or \"math\""))),
    }
}

/// Copies `from` over `to` through a temporary file, and only keeps it when its size and
/// SHA-256 match the manifest. A failed check leaves the previous file (or nothing) in place.
fn copy_verified(from: &Path, to: &Path, f: &ModelFile) -> Result<()> {
    if let Some(dir) = to.parent() {
        std::fs::create_dir_all(dir).map_err(|e| bad("ink.modelImport", format!("{}: {e}", dir.display())))?;
    }
    let part = to.with_extension("part");
    let src_size = std::fs::metadata(from).map_err(|e| bad("ink.modelImport", format!("{}: {e}", from.display())))?.len();
    if src_size != f.bytes {
        return Err(bad("ink.modelImport", format!("{} is {src_size} bytes, expected {} for {}", from.display(), f.bytes, f.path)));
    }
    std::fs::copy(from, &part).map_err(|e| bad("ink.modelImport", format!("{}: {e}", from.display())))?;
    let check = (|| {
        let size = std::fs::metadata(&part).map_err(|e| bad("ink.modelImport", format!("{}: {e}", part.display())))?.len();
        if size != f.bytes {
            return Err(bad("ink.modelImport", format!("{} is {size} bytes, expected {} for {}", from.display(), f.bytes, f.path)));
        }
        let hash = gridcraft_ink::manifest::sha256_file(&part).map_err(|e| bad("ink.modelImport", e.to_string()))?;
        if !hash.eq_ignore_ascii_case(f.sha256) {
            return Err(bad(
                "ink.modelImport",
                format!("{} is not the file the manifest names for {} (SHA-256 {hash}); check the download", from.display(), f.path),
            ));
        }
        Ok(())
    })();
    if check.is_err() {
        let _ = std::fs::remove_file(&part);
        return check;
    }
    std::fs::rename(&part, to).map_err(|e| {
        let _ = std::fs::remove_file(&part);
        bad("ink.modelImport", format!("{}: {e}", to.display()))
    })
}

/// `ink.modelImport {path, model?}`: copies a model file (or a folder of them) into the model
/// directory, verifying each one. Importing the files one at a time works; the answer says what
/// is missing. `model: "math"` imports the Ink to Math model; the handwriting model is default.
fn model_import(s: &mut Session, p: &Json) -> Result<Json> {
    let path = str_param(p, "path")
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| bad("ink.modelImport", "`path` is required: a folder holding the model's files, or one of the files"))?;
    let Some(root) = s.models_dir.as_deref().map(PathBuf::from) else {
        return Err(bad("ink.modelImport", "No model directory is configured. The desktop app sets one; `models/` beside the executable works too."));
    };
    let m = manifest_param(p, "ink.modelImport")?;
    let src = PathBuf::from(path);
    let dir = m.dir(&root);
    let mut copied: Vec<&str> = Vec::new();
    if src.is_dir() {
        for f in m.files {
            let name = f.path.rsplit('/').next().unwrap_or(f.path);
            // The folder as laid out upstream, or with the files side by side.
            let from = [src.join(f.path), src.join(name)].into_iter().find(|c| c.is_file());
            let Some(from) = from else { continue };
            copy_verified(&from, &dir.join(f.path), f)?;
            copied.push(f.path);
        }
        if copied.is_empty() {
            return Err(bad(
                "ink.modelImport",
                format!("{} holds none of the model's files ({})", src.display(), m.files.iter().map(|f| f.path).collect::<Vec<_>>().join(", ")),
            ));
        }
    } else if src.is_file() {
        let name = src.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        let f = m.file_named(name).ok_or_else(|| {
            bad(
                "ink.modelImport",
                format!("`{name}` is not one of the model's files ({})", m.files.iter().map(|f| f.path).collect::<Vec<_>>().join(", ")),
            )
        })?;
        copy_verified(&src, &dir.join(f.path), f)?;
        copied.push(f.path);
    } else {
        return Err(bad("ink.modelImport", format!("{}: no such file or folder", src.display())));
    }
    let state = m.check_sizes(&root);
    let installed = state.is_installed();
    let missing = match state {
        InstallState::Incomplete(files) => files,
        _ => Vec::new(),
    };
    Ok(json!({"copied": copied, "installed": installed, "missing": missing, "directory": dir.display().to_string()}))
}

/// `ink.modelRemove {model?}`: deletes the model's directory and turns its tool off.
fn model_remove(s: &mut Session, p: &Json) -> Result<Json> {
    let Some(root) = s.models_dir.as_deref().map(PathBuf::from) else {
        return Err(bad("ink.modelRemove", "No model directory is configured."));
    };
    let m = manifest_param(p, "ink.modelRemove")?;
    let dir = m.dir(&root);
    if !dir.is_dir() {
        let which = if m.output == RecognitionOutput::Math { "math" } else { "handwriting" };
        return Err(bad("ink.modelRemove", format!("The {which} model is not installed in {}.", dir.display())));
    }
    std::fs::remove_dir_all(&dir).map_err(|e| bad("ink.modelRemove", format!("{}: {e}", dir.display())))?;
    if m.output == RecognitionOutput::Math {
        s.ink_math = None;
    } else {
        s.ink = None;
        s.ink_entry = None;
        if s.draw_tool == "actionPen" {
            s.draw_tool = String::new();
        }
    }
    Ok(json!({"removed": dir.display().to_string()}))
}

fn parse_points(t: &str) -> Vec<[f32; 2]> {
    t.split(';').filter_map(|pt| pt.split_once(',').and_then(|(x, y)| Some([x.parse().ok()?, y.parse().ok()?]))).collect()
}

/// `{points: [[x, y], …]}` as sheet points: non-finite pairs dropped, at most 20,000 kept.
fn points_param(p: &Json) -> Vec<[f64; 2]> {
    p.get("points")
        .and_then(Json::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|q| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?]))
                .filter(|q| q[0].is_finite() && q[1].is_finite())
                .take(20_000)
                .collect()
        })
        .unwrap_or_default()
}

fn stroke(s: &mut Session, p: &Json) -> Result<Json> {
    let pts: Vec<[f64; 2]> = points_param(p);
    if pts.len() < 2 {
        return Err(bad("draw.stroke", "a stroke needs at least two points"));
    }
    let minx = pts.iter().map(|q| q[0]).fold(f64::INFINITY, f64::min).max(0.0);
    let miny = pts.iter().map(|q| q[1]).fold(f64::INFINITY, f64::min).max(0.0);
    let maxx = pts.iter().map(|q| q[0]).fold(f64::NEG_INFINITY, f64::max);
    let maxy = pts.iter().map(|q| q[1]).fold(f64::NEG_INFINITY, f64::max);
    let color =
        str_param(p, "color").map(str::to_string).unwrap_or_else(|| if s.draw_color.is_empty() { "#1F5FC9".into() } else { s.draw_color.clone() });
    let width = f64_param(p, "width").map(|w| w as f32).unwrap_or(if s.draw_width <= 0.0 { 2.0 } else { s.draw_width }).clamp(0.5, 40.0);
    let d = s.doc()?;
    let si = d.wb.active_sheet();
    let sh = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    let cell = CellRef::new(sh.row_at(miny), sh.col_at(minx));
    let dx = (minx - sh.col_left(cell.col)) as f32;
    let dy = (miny - sh.row_top(cell.row)) as f32;
    let text = pts.iter().map(|q| format!("{:.1},{:.1}", q[0] - minx, q[1] - miny)).collect::<Vec<_>>().join(";");
    let id = d.wb.next_object_id();
    let shape = Shape {
        math: Default::default(),
        id,
        kind: ShapeKind::Ink,
        anchor: Anchor {
            cell,
            dx,
            dy,
            width: (maxx - minx).max(1.0) as f32,
            height: (maxy - miny).max(1.0) as f32,
            // Ink is a freehand mark: pinned where it was drawn, it does not follow cells.
            mode: gridcraft_model::AnchorMode::Absolute,
        },
        fill: Color::from_hex(&format!("#{:02X}0000", width.round().clamp(0.0, 255.0) as u8)).unwrap_or_default(),
        line: Color::from_hex(&color).unwrap_or(Color::rgb(0x1F, 0x5F, 0xC9)),
        text,
        format: Default::default(),
    };
    edit(s, |cx| {
        cx.sheet_mut(si)?.shapes.push(shape);
        Ok(json!({"shape": id}))
    })
}

/// Ink width is stored in the red channel of `fill` (shapes have no width field).
pub fn ink_width(sh: &Shape) -> f32 {
    match sh.fill {
        Color::Rgb(v) => ((v >> 16) & 0xFF).max(1) as f32,
        _ => 2.0,
    }
}

pub fn ink_points(sh: &Shape) -> Vec<[f32; 2]> {
    parse_points(&sh.text)
}

/// `points` as one stroke (`[[x, y], …]`) or several (`[[[x, y], …], …]`): non-finite pairs
/// dropped, each stroke capped at 20,000 points and at most 32 strokes kept, like an Action Pen
/// entry. `points` is input, so anything else is ignored rather than trusted.
fn strokes_param(p: &Json) -> Vec<Vec<[f64; 2]>> {
    fn one(a: &[Json]) -> Vec<[f64; 2]> {
        a.iter()
            .filter_map(|q| Some([q.get(0)?.as_f64()?, q.get(1)?.as_f64()?]))
            .filter(|q| q[0].is_finite() && q[1].is_finite())
            .take(gridcraft_ink::raster::MAX_POINTS)
            .collect()
    }
    let Some(items) = p.get("points").and_then(Json::as_array) else { return Vec::new() };
    let nested = items.first().and_then(Json::as_array).and_then(|a| a.first()).is_some_and(Json::is_array);
    if nested {
        items.iter().filter_map(Json::as_array).take(MAX_ENTRY_STROKES).map(|a| one(a)).filter(|s| !s.is_empty()).collect()
    } else {
        let stroke = one(items);
        if stroke.is_empty() { Vec::new() } else { vec![stroke] }
    }
}

/// What `draw.inkToMath` recognises, and what to do with the sheet once the equation is ready.
struct MathInk {
    /// The strokes in sheet points, one entry per stroke; they are never joined.
    strokes: Vec<Vec<[f32; 2]>>,
    /// Where the equation goes (the ink's anchor) and, for supplied points, the box they cover.
    anchor: Anchor,
    /// The equation's ink colour: the stroke's, or Automatic for supplied points.
    color: Color,
    /// The pen width the strokes are rasterised at.
    pen: f32,
    /// Ink shapes to remove once the equation is in place; empty for supplied points.
    consumed: Vec<u32>,
}

/// The ink ids a command names: `ids` (a list, honoured even when empty), else `id`, else none.
fn ink_ids_param(p: &Json) -> Option<Vec<u32>> {
    if let Some(list) = p.get("ids").and_then(Json::as_array) {
        return Some(list.iter().filter_map(Json::as_u64).map(|v| v.min(u32::MAX as u64) as u32).take(MAX_ENTRY_STROKES).collect());
    }
    u32_param(p, "id").map(|id| vec![id])
}

/// `draw.inkToMath {id?|ids?, points?}`: recognises handwritten ink as a formula and replaces it
/// with an equation holding the LaTeX the math model returns, at the ink's anchor. One undo step;
/// the ink is consumed only once the equation is ready, so a refused recognition leaves the sheet
/// exactly as it was. Without the math model the command refuses with the folder to install it in
/// (the UI opens the model panel instead).
///
/// `ids` (or `id`) names the strokes — the UI passes the selected ink; without either the last
/// stroke on the sheet is used, as `draw.inkToShape` does. `points` recognises strokes supplied
/// directly and consumes nothing, which is what a script, MCP or a test uses.
fn ink_to_math(s: &mut Session, p: &Json) -> Result<Json> {
    const CMD: &str = "draw.inkToMath";
    let Some(recognizer) = s.ink_math.clone().filter(|r| r.output() == RecognitionOutput::Math) else {
        return Err(no_math_recognizer(s, CMD));
    };
    let explicit = strokes_param(p);
    let named = ink_ids_param(p);
    let d = s.doc()?;
    let si = d.wb.active_sheet();
    let sheet = d.wb.sheet(si).ok_or(EngineError::NoDocument)?;
    if sheet.is_protected() {
        return Err(EngineError::Other(PROTECTED.into()));
    }
    // Explicit points are used as drawn; named ink becomes absolute sheet points so several
    // strokes (a fraction bar and its parts, an "=") keep their relative positions.
    let ink = if !explicit.is_empty() {
        let (minx, miny) = explicit.iter().flatten().fold((f64::INFINITY, f64::INFINITY), |(x, y), q| (x.min(q[0]), y.min(q[1])));
        let (maxx, maxy) = explicit.iter().flatten().fold((f64::NEG_INFINITY, f64::NEG_INFINITY), |(x, y), q| (x.max(q[0]), y.max(q[1])));
        let cell = CellRef::new(sheet.row_at(miny.max(0.0)), sheet.col_at(minx.max(0.0)));
        MathInk {
            strokes: explicit.iter().map(|stroke| stroke.iter().map(|q| [q[0] as f32, q[1] as f32]).collect()).collect(),
            anchor: Anchor {
                cell,
                dx: (minx - sheet.col_left(cell.col)) as f32,
                dy: (miny - sheet.row_top(cell.row)) as f32,
                width: (maxx - minx).max(1.0) as f32,
                height: (maxy - miny).max(1.0) as f32,
                mode: AnchorMode::MoveOnly,
            },
            color: Color::Auto,
            pen: if s.draw_width <= 0.0 { 2.0 } else { s.draw_width },
            consumed: Vec::new(),
        }
    } else {
        let picked: Vec<&Shape> = match named {
            Some(ids) => ids.iter().filter_map(|id| sheet.shapes.iter().find(|x| x.id == *id && x.kind == ShapeKind::Ink)).collect(),
            None => sheet.shapes.iter().rfind(|x| x.kind == ShapeKind::Ink).into_iter().collect(),
        };
        let Some(first) = picked.first() else {
            return Err(bad(CMD, "no ink stroke is selected to convert into an equation"));
        };
        let (left, top) = (sheet.col_left(first.anchor.cell.col), sheet.row_top(first.anchor.cell.row));
        let (ox, oy) = (left + first.anchor.dx as f64, top + first.anchor.dy as f64);
        MathInk {
            strokes: picked
                .iter()
                .map(|sh| ink_points(sh).iter().map(|q| [(ox + q[0] as f64) as f32, (oy + q[1] as f64) as f32]).collect())
                .collect(),
            anchor: Anchor {
                cell: first.anchor.cell,
                dx: first.anchor.dx,
                dy: first.anchor.dy,
                width: first.anchor.width,
                height: first.anchor.height,
                mode: AnchorMode::MoveOnly,
            },
            color: first.line,
            pen: ink_width(first),
            consumed: picked.iter().map(|sh| sh.id).collect(),
        }
    };
    let MathInk { strokes, anchor, color, pen, consumed } = ink;
    let image = {
        let refs: Vec<&[[f32; 2]]> = strokes.iter().map(Vec::as_slice).collect();
        gridcraft_ink::raster::rasterize_strokes(&refs, pen).map_err(|e| bad(CMD, e.to_string()))
    }?;
    let latex = recognizer.recognize(&image).map_err(|e| bad(CMD, e.to_string()))?.trim().to_string();
    if latex.is_empty() {
        return Err(bad(CMD, "Nothing was recognised as a formula. Write larger, or try again."));
    }
    if latex.len() > gridcraft_math::MAX_SOURCE_BYTES {
        return Err(bad(
            CMD,
            format!(
                "The recognised formula is too long: {} bytes, over the {} the equation can hold.",
                latex.len(),
                gridcraft_math::MAX_SOURCE_BYTES
            ),
        ));
    }
    // Laid out and validated before anything changes: a model that answers something the layout
    // engine refuses must not consume the ink.
    let d = s.doc()?;
    let id = d.wb.next_object_id();
    let shape = super::equation::build_equation(CMD, id, &latex, anchor, color, super::equation::DEFAULT_SIZE_PT)?;
    let si = d.wb.active_sheet();
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        sh.shapes.retain(|x| !consumed.contains(&x.id));
        sh.shapes.push(shape);
        Ok(Json::Null)
    })?;
    Ok(json!({"equation": id, "latex": latex}))
}

fn ink_to_shape(s: &mut Session, p: &Json) -> Result<Json> {
    let si = s.doc()?.wb.active_sheet();
    let id = u32_param(p, "id");
    edit(s, |cx| {
        let sh = cx.sheet_mut(si)?;
        let idx = match id {
            Some(id) => sh.shapes.iter().position(|x| x.id == id && x.kind == ShapeKind::Ink),
            None => sh.shapes.iter().rposition(|x| x.kind == ShapeKind::Ink),
        }
        .ok_or_else(|| bad("draw.inkToShape", "no ink stroke to convert"))?;
        let ink = sh.shapes.get_mut(idx).ok_or_else(|| bad("draw.inkToShape", "gone"))?;
        let pts = parse_points(&ink.text);
        let (w, h) = (ink.anchor.width.max(1.0), ink.anchor.height.max(1.0));
        let closed = match (pts.first(), pts.last()) {
            (Some(a), Some(b)) => ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() < 0.25 * w.max(h),
            _ => false,
        };
        let kind = if !closed {
            ShapeKind::Line
        } else {
            // Corners hugging the bounding box mean a rectangle; one apex at the top a triangle.
            let near = |cx: f32, cy: f32| pts.iter().any(|q| ((q[0] - cx).powi(2) + (q[1] - cy).powi(2)).sqrt() < 0.15 * w.min(h).max(8.0));
            let corners = [near(0.0, 0.0), near(w, 0.0), near(0.0, h), near(w, h)].iter().filter(|b| **b).count();
            let apex = near(w / 2.0, 0.0);
            if corners >= 3 {
                ShapeKind::Rectangle
            } else if apex && near(0.0, h) && near(w, h) {
                ShapeKind::Triangle
            } else {
                ShapeKind::Ellipse
            }
        };
        let color = ink.line;
        ink.kind = kind;
        ink.fill = if kind == ShapeKind::Line { color } else { Color::Auto };
        ink.text = String::new();
        if kind == ShapeKind::Line && pts.first().zip(pts.last()).is_some_and(|(a, b)| a[1] > b[1]) {
            // Lines are drawn top-left to bottom-right; keep the stroke direction simple.
        }
        Ok(json!({"kind": format!("{kind:?}")}))
    })
}
