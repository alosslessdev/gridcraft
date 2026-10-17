//! Handwriting recognition for the Action Pen through the Windows Ink service.
//!
//! Excel documents an offline fallback to the Windows handwriting experience; GridCraft uses the
//! same OS API (`Windows.UI.Input.Inking`). Nothing from Office is involved: the strokes are handed
//! to the platform recognizer and the recognized text enters the cell through the ordinary
//! `cell.set` path.

use std::time::Duration;

use gridcraft_engine::handwriting::HandwritingRecognizer;

pub struct WindowsHandwriting;

impl HandwritingRecognizer for WindowsHandwriting {
    fn recognize(&self, strokes: &[Vec<[f32; 2]>], language: &str) -> Result<String, String> {
        // Run the recognizer on a worker thread. The UI thread is an STA (winit initializes OLE for
        // drag and drop), and blocking an STA on WinRT's async recognition deadlocks: the completion
        // is delivered through that thread's message loop. A worker joins the implicit MTA, where
        // blocking is safe; the wait is bounded so a stuck recognizer cannot freeze the app.
        let (tx, rx) = std::sync::mpsc::channel();
        let strokes = strokes.to_vec();
        let language = language.to_string();
        std::thread::spawn(move || {
            let _ = tx.send(recognize(&strokes, &language).map_err(|e| e.message().to_string()));
        });
        match rx.recv_timeout(Duration::from_secs(10)) {
            Ok(result) => result,
            Err(_) => Err("handwriting recognition timed out".into()),
        }
    }
}

fn recognize(strokes: &[Vec<[f32; 2]>], language: &str) -> windows::core::Result<String> {
    use std::future::IntoFuture;
    use windows::Foundation::Point;
    use windows::UI::Input::Inking::{InkPoint, InkRecognitionTarget, InkRecognizerContainer, InkStrokeBuilder, InkStrokeContainer};
    use windows_numerics::Matrix3x2;

    let scaled = pen_sized(strokes);
    let builder = InkStrokeBuilder::new()?;
    let container = InkStrokeContainer::new()?;
    for stroke in scaled.iter().filter(|stroke| stroke.len() >= 2).take(8) {
        // Timestamp and pressure like a pen: the recognizer ranks a bare point list as punctuation
        // (a zero-duration mark), while sampled ink with timing reads as writing.
        let points: Vec<Option<InkPoint>> = stroke
            .iter()
            .take(20_000)
            .enumerate()
            .filter_map(|(i, p)| InkPoint::CreateInkPointWithTiltAndTimestamp(Point { X: p[0], Y: p[1] }, 0.5, 0.0, 0.0, i as u64 * 8_000).ok())
            .map(Some)
            .collect();
        let iterable = windows_collections::IIterable::<InkPoint>::from(points);
        container.AddStroke(&builder.CreateStrokeFromInkPoints(&iterable, Matrix3x2::identity())?)?;
    }
    let recognizer = InkRecognizerContainer::new()?;
    // Pick the recognizer for the requested language (the container's default follows the system
    // locale, which need not be the workbook's language).
    if let Ok(list) = recognizer.GetRecognizers() {
        let want = if language.is_empty() { "" } else { language };
        for k in 0..list.Size()? {
            let candidate = list.GetAt(k)?;
            let name = candidate.Name()?.to_string();
            let matches = want.is_empty()
                || (want.starts_with("en") && name.contains("English"))
                || (want.starts_with("es") && (name.contains("Español") || name.contains("Spanish")));
            if matches {
                recognizer.SetDefaultRecognizer(&candidate)?;
                break;
            }
        }
    }
    let results = pollster::block_on(recognizer.RecognizeAsync(&container, InkRecognitionTarget::All)?.into_future())?;
    let mut out = String::new();
    for i in 0..results.Size()? {
        let candidates = results.GetAt(i)?.GetTextCandidates()?;
        if candidates.Size()? > 0 {
            // The platform orders candidates by confidence; the first is the recognized text.
            let text = candidates.GetAt(0)?.to_string();
            if !out.is_empty() && !text.is_empty() {
                out.push(' ');
            }
            out.push_str(&text);
        }
    }
    Ok(out)
}

/// Windows Ink expects pen-sized input; a word written inside a few cells is only tens of points,
/// which the recognizer reads as punctuation. Scale the ink about its origin to a comfortable
/// writing size (its longest side becomes ~240 units).
fn pen_sized(strokes: &[Vec<[f32; 2]>]) -> Vec<Vec<[f32; 2]>> {
    let ((minx, miny), (maxx, maxy)) =
        strokes.iter().flatten().fold(((f32::INFINITY, f32::INFINITY), (f32::NEG_INFINITY, f32::NEG_INFINITY)), |((minx, miny), (maxx, maxy)), p| {
            ((minx.min(p[0]), miny.min(p[1])), (maxx.max(p[0]), maxy.max(p[1])))
        });
    if !minx.is_finite() || !miny.is_finite() {
        return strokes.to_vec();
    }
    let extent = (maxx - minx).max(maxy - miny).max(1.0);
    let scale = (240.0 / extent).clamp(1.0, 12.0);
    strokes.iter().map(|stroke| stroke.iter().map(|p| [(p[0] - minx) * scale, (p[1] - miny) * scale]).collect()).collect()
}
