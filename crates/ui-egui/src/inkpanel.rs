//! Draw › Action Pen and Draw › Convert › Ink to Math: handwriting to text and handwriting to an
//! equation.
//!
//! Two things live here. [`start`] and [`poll`] wire the host's model directory to the engine's
//! [`gridcraft_ink::Recognizer`]s: at startup when the files are already there, and after a
//! download or an import. [`show`] is the model panel the Draw tab opens when a model is absent:
//! each model's name, licence, size, folder and status, with Download, Import… and Remove.
//!
//! Nothing here runs at startup beyond a `stat` per model file: no network call, no model read
//! until a load is asked for (`docs/action-pen.md`, `docs/ink-to-math.md`).

use std::sync::Arc;
use std::sync::mpsc::Receiver;

use gridcraft_ink::{Manifest, PIX2TEXT_MFR, Recognizer, TROCR_SMALL_HANDWRITTEN};
use serde_json::{Value, json};

use crate::SheetApp;

/// What a worker thread finished: a model download, or loading the recognizer.
pub enum Job {
    Downloaded(Result<(), String>),
    Loaded(Result<Arc<dyn Recognizer>, String>),
}

/// The two models the Draw tab can use. They are independent: either, both or neither may be
/// installed, and each has its own status, worker job and error message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Draw › Action Pen: handwriting to text.
    Text,
    /// Draw › Convert › Ink to Math: handwriting to LaTeX.
    Math,
}

impl Kind {
    const ALL: [Kind; 2] = [Kind::Text, Kind::Math];

    fn manifest(self) -> &'static Manifest {
        match self {
            Kind::Text => &TROCR_SMALL_HANDWRITTEN,
            Kind::Math => &PIX2TEXT_MFR,
        }
    }

    /// The name `ink.modelImport`, `ink.modelRemove` and the status commands use.
    fn key(self) -> &'static str {
        match self {
            Kind::Text => "text",
            Kind::Math => "math",
        }
    }

    /// The panel's heading.
    fn title(self) -> &'static str {
        match self {
            Kind::Text => "Action Pen — handwriting to text",
            Kind::Math => "Ink to Math — handwriting to an equation",
        }
    }

    /// What the model does, in one line.
    fn blurb(self) -> &'static str {
        match self {
            Kind::Text => "Write in a cell with the Action Pen and the writing is typed into it.",
            Kind::Math => "Select handwritten ink and Draw › Convert › Ink to Math turns it into an equation.",
        }
    }

    /// The query that answers this model's status.
    fn status_command(self) -> &'static str {
        match self {
            Kind::Text => "ink.status",
            Kind::Math => "ink.mathStatus",
        }
    }

    /// Whether the engine slot holds a recognizer of the right kind.
    fn recognizer(self, app: &SheetApp) -> Option<&Arc<dyn Recognizer>> {
        let slot = match self {
            Kind::Text => app.session.ink.as_ref(),
            Kind::Math => app.session.ink_math.as_ref(),
        };
        slot.filter(|r| r.output() == self.manifest().output)
    }
}

/// Starts the models the host has a directory for: sets `Session::models_dir` and loads each
/// recognizer whose files are present, on a worker thread.
pub fn start(app: &mut SheetApp) {
    let Some(host) = app.services.models.clone() else {
        app.session.ink = None;
        app.session.ink_math = None;
        return;
    };
    let root = host.models_dir();
    app.session.models_dir = Some(root.display().to_string());
    for kind in Kind::ALL {
        if kind.recognizer(app).is_none() && kind.manifest().check_sizes(&root).is_installed() {
            load(app, kind);
        }
    }
}

/// Loads one model from the configured model directory on a worker thread, replacing any
/// recognizer already there when it succeeds.
fn load(app: &mut SheetApp, kind: Kind) {
    let loader = match kind {
        Kind::Text => app.services.ink_loader.clone(),
        Kind::Math => app.services.math_loader.clone(),
    };
    let (Some(loader), Some(root)) = (loader, app.session.models_dir.clone()) else { return };
    if job(app, kind).is_some() {
        return;
    }
    let path = std::path::PathBuf::from(root);
    *job_mut(app, kind) = spawn(move || Job::Loaded(loader(&path)));
    if job(app, kind).is_none() {
        set_error(app, kind, Some(model_cannot_load(kind).to_string()));
    }
}

fn model_cannot_load(kind: Kind) -> &'static str {
    match kind {
        Kind::Text => "The handwriting model can't be loaded in this build.",
        Kind::Math => "The math model can't be loaded in this build.",
    }
}

/// Runs `job` on a worker and returns the channel its answer arrives on. Without threads (the
/// web app) there is nothing to run: every model job needs a native host.
#[cfg(not(target_arch = "wasm32"))]
fn spawn<F: FnOnce() -> Job + Send + 'static>(job: F) -> Option<Receiver<Job>> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(job());
    });
    Some(rx)
}

#[cfg(target_arch = "wasm32")]
fn spawn<F: FnOnce() -> Job + Send + 'static>(_job: F) -> Option<Receiver<Job>> {
    None
}

/// Answers a finished worker job. Called every frame; cheap when nothing is running.
///
/// A worker runs outside the frame loop, so while a job is in flight the UI is kept awake: with
/// the model panel closed nothing else would repaint, and a finished download would wait for an
/// unrelated event before the recognizer starts loading.
pub fn poll(app: &mut SheetApp, ctx: &egui::Context) {
    let mut waiting = false;
    for kind in Kind::ALL {
        let Some(rx) = job_mut(app, kind).take() else { continue };
        waiting = true;
        match rx.try_recv() {
            Ok(job) => apply(app, kind, job),
            Err(std::sync::mpsc::TryRecvError::Empty) => *job_mut(app, kind) = Some(rx),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                let message = match kind {
                    Kind::Text => "The handwriting model download stopped unexpectedly. Try again.",
                    Kind::Math => "The math model download stopped unexpectedly. Try again.",
                };
                set_error(app, kind, Some(message.to_string()));
            }
        }
    }
    if waiting {
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }
}

/// Applies one finished job to the right model.
fn apply(app: &mut SheetApp, kind: Kind, answer: Job) {
    match answer {
        Job::Downloaded(Ok(())) => {
            set_error(app, kind, None);
            refresh(app);
            load(app, kind);
        }
        Job::Downloaded(Err(e)) => set_error(app, kind, Some(e)),
        Job::Loaded(Ok(recognizer)) => {
            set_error(app, kind, None);
            set_recognizer(app, kind, Some(recognizer));
            refresh(app);
        }
        Job::Loaded(Err(e)) => {
            set_recognizer(app, kind, None);
            set_error(app, kind, Some(e));
            refresh(app);
        }
    }
}

/// Asks the engine for both models' status (three `stat` calls each; no reading, no hashing).
fn refresh(app: &mut SheetApp) {
    for kind in Kind::ALL {
        if let Ok(status) = app.run(kind.status_command(), json!({})) {
            *status_mut(app, kind) = Some(status);
        }
    }
}

/// Opens the model panel (a Draw tab button when its model can't be used).
pub fn open(app: &mut SheetApp) {
    app.grid.ink_panel = true;
    app.grid.ink_status = None;
    app.grid.math_status = None;
    refresh(app);
}

/// Whether the engine has the handwriting recognizer, so the Draw tab can show Action Pen as
/// available.
pub fn ready(app: &SheetApp) -> bool {
    Kind::Text.recognizer(app).is_some()
}

/// Whether the engine has the math recognizer, so Draw › Convert › Ink to Math can run.
pub fn math_ready(app: &SheetApp) -> bool {
    Kind::Math.recognizer(app).is_some()
}

/// The model panel, while it is open.
pub fn show(app: &mut SheetApp, ctx: &egui::Context) {
    if !app.grid.ink_panel {
        return;
    }
    if app.grid.ink_status.is_none() || app.grid.math_status.is_none() {
        refresh(app);
    }
    let mut open = true;
    egui::Window::new("Ink models").open(&mut open).collapsible(false).resizable(false).default_width(460.0).show(ctx, |ui| panel(app, ui));
    if !open {
        app.grid.ink_panel = false;
    }
}

/// What the panel's buttons ask the app to do for one model.
enum Action {
    None,
    Download,
    Import,
    Remove,
}

fn panel(app: &mut SheetApp, ui: &mut egui::Ui) {
    for (i, kind) in Kind::ALL.into_iter().enumerate() {
        if i > 0 {
            ui.separator();
        }
        // Clone what the section reads, so the app can be borrowed mutably to apply its action.
        let status = status(app, kind).cloned().unwrap_or_else(|| json!({}));
        let error = error(app, kind).cloned();
        let busy = job(app, kind).is_some();
        let has_host = app.services.models.is_some();
        let has_picker = app.services.pick_model_files.is_some();
        ui.heading(kind.title());
        ui.label(kind.blurb());
        ui.add_space(6.0);
        let action = section(ui, kind, &status, error.as_deref(), busy, has_host, has_picker);
        apply_action(app, kind, action);
    }
}

/// One model's section: its provenance, folder and status, and the three buttons. Reads only,
/// so the caller can apply the returned action with the session borrowed mutably.
fn section(ui: &mut egui::Ui, kind: Kind, status: &Value, error: Option<&str>, busy: bool, has_host: bool, has_picker: bool) -> Action {
    let manifest = kind.manifest();
    let ready = status["ready"].as_bool().unwrap_or(false);
    let present = status["filesPresent"].as_bool().unwrap_or(false);
    let directory = status["directory"].as_str().unwrap_or_default().to_string();
    let missing: Vec<String> =
        status["missing"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()).unwrap_or_default();

    egui::Grid::new(format!("ink-model-{}", kind.key())).num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.strong("Model");
        ui.label(format!("{} ({})", manifest.name, manifest.id));
        ui.end_row();
        ui.strong("Licence");
        ui.label(manifest.licence);
        ui.end_row();
        ui.strong("Size");
        ui.label(format!("{:.0} MB in {} files", manifest.total_bytes() as f64 / 1_000_000.0, manifest.files.len()));
        ui.end_row();
        ui.strong("Source");
        ui.add(egui::Label::new(egui::RichText::new(manifest.source).monospace()).truncate());
        ui.end_row();
        ui.strong("Folder");
        ui.add(egui::Label::new(egui::RichText::new(&directory).monospace()).truncate());
        ui.end_row();
        ui.strong("Runs on");
        ui.label(status["accelerator"].as_str().unwrap_or("cpu").to_uppercase());
        ui.end_row();
        ui.strong("Status");
        let (text, color) = if ready {
            ("Ready".to_string(), ui.visuals().hyperlink_color)
        } else if present {
            ("Files present, not loaded".to_string(), ui.visuals().warn_fg_color)
        } else {
            ("Not installed".to_string(), ui.visuals().warn_fg_color)
        };
        ui.label(egui::RichText::new(text).color(color));
        ui.end_row();
    });
    if !missing.is_empty() {
        ui.add_space(4.0);
        ui.label(format!("Missing: {}", missing.join(", ")));
    }
    if let Some(hint) = status["hint"].as_str() {
        ui.add_space(4.0);
        ui.label(egui::RichText::new(hint).italics());
    }
    if let Some(err) = error {
        ui.add_space(4.0);
        ui.colored_label(ui.visuals().error_fg_color, err);
    }
    ui.add_space(8.0);

    let action = ui
        .horizontal(|ui| {
            let size = manifest.total_bytes() / 1_000_000;
            let download = ui
                .add_enabled(has_host && !busy && !ready, egui::Button::new("Download"))
                .on_hover_text(format!("Download the model from its published source (about {size} MB)"));
            if download.clicked() {
                return Action::Download;
            }
            let import = ui.add_enabled(has_picker && !busy && !ready, egui::Button::new("Import…"));
            if import.clicked() {
                return Action::Import;
            }
            let remove = ui.add_enabled(present && !busy, egui::Button::new("Remove"));
            if remove.clicked() {
                return Action::Remove;
            }
            if busy {
                ui.spinner();
            }
            Action::None
        })
        .inner;
    if !has_host {
        ui.add_space(4.0);
        ui.label("Downloading is available in the desktop app. You can copy the model's files into the folder above and use Import… .");
    }
    ui.add_space(4.0);
    ui.label(
        egui::RichText::new(format!(
            "The model is not bundled with GridCraft. {} — {} — about {} MB.",
            manifest.licence,
            manifest.source,
            manifest.total_bytes() / 1_000_000
        ))
        .small(),
    );
    // `Action::None` means no button was pressed; anything else is the action to apply.
    action
}

/// Applies one section's action to the model it belongs to.
fn apply_action(app: &mut SheetApp, kind: Kind, action: Action) {
    match action {
        Action::None => {}
        Action::Download => {
            let Some(host) = app.services.models.clone() else { return };
            set_error(app, kind, None);
            let id = kind.manifest().id;
            *job_mut(app, kind) = spawn(move || Job::Downloaded(host.download(id)));
            if job(app, kind).is_none() {
                set_error(app, kind, Some("Downloads need the desktop app.".to_string()));
            }
        }
        Action::Import => {
            let picked = app.services.pick_model_files.as_ref().map(|pick| pick()).unwrap_or_default();
            if picked.is_empty() {
                return;
            }
            set_error(app, kind, None);
            let mut installed = false;
            for path in picked {
                match app.run("ink.modelImport", json!({"path": path, "model": kind.key()})) {
                    Ok(r) => installed = r["installed"].as_bool().unwrap_or(false) || installed,
                    Err(e) => {
                        set_error(app, kind, Some(e));
                        break;
                    }
                }
            }
            if installed {
                load(app, kind);
            }
            refresh(app);
        }
        Action::Remove => match app.run("ink.modelRemove", json!({"model": kind.key()})) {
            Ok(_) => {
                set_recognizer(app, kind, None);
                set_error(app, kind, None);
                refresh(app);
            }
            Err(e) => set_error(app, kind, Some(e)),
        },
    }
}

// ---------------------------------------------------------------- the two models' state

fn status(app: &SheetApp, kind: Kind) -> Option<&Value> {
    match kind {
        Kind::Text => app.grid.ink_status.as_ref(),
        Kind::Math => app.grid.math_status.as_ref(),
    }
}

fn status_mut(app: &mut SheetApp, kind: Kind) -> &mut Option<Value> {
    match kind {
        Kind::Text => &mut app.grid.ink_status,
        Kind::Math => &mut app.grid.math_status,
    }
}

fn error(app: &SheetApp, kind: Kind) -> Option<&String> {
    match kind {
        Kind::Text => app.grid.ink_error.as_ref(),
        Kind::Math => app.grid.math_error.as_ref(),
    }
}

fn set_error(app: &mut SheetApp, kind: Kind, message: Option<String>) {
    match kind {
        Kind::Text => app.grid.ink_error = message,
        Kind::Math => app.grid.math_error = message,
    }
}

fn job(app: &SheetApp, kind: Kind) -> Option<&Receiver<Job>> {
    match kind {
        Kind::Text => app.grid.ink_job.as_ref(),
        Kind::Math => app.grid.math_job.as_ref(),
    }
}

fn job_mut(app: &mut SheetApp, kind: Kind) -> &mut Option<Receiver<Job>> {
    match kind {
        Kind::Text => &mut app.grid.ink_job,
        Kind::Math => &mut app.grid.math_job,
    }
}

fn set_recognizer(app: &mut SheetApp, kind: Kind, recognizer: Option<Arc<dyn Recognizer>>) {
    match kind {
        Kind::Text => app.session.ink = recognizer,
        Kind::Math => app.session.ink_math = recognizer,
    }
}
