//! Handwriting recognition supplied by the platform.

/// Recognizes handwriting from ink strokes. The engine never links a recognizer itself: the app
/// registers the platform's service on [`crate::Session::handwriting`], so headless, macOS, Linux
/// and wasm builds stay portable and the command simply reports that recognition is unavailable.
pub trait HandwritingRecognizer: Send + Sync {
    /// Recognize the strokes (each a list of `[x, y]` sheet points). `language` is a BCP-47 tag,
    /// or empty for the platform default.
    fn recognize(&self, strokes: &[Vec<[f32; 2]>], language: &str) -> Result<String, String>;
}
