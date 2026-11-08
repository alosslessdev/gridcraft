//! Real-model evidence for Draw › Convert › Ink to Math, run by hand (never in CI: it needs the
//! 112 MB model, which is not committed).
//!
//! ```text
//! GRIDCRAFT_PIX2TEXT_MODEL=<models root> \
//!   cargo test --release -p gridcraft-ink-rten --test real_math_model -- --ignored --nocapture
//! ```
//!
//! `<models root>` holds `pix2text-mfr/` (`encoder_model.onnx`, `decoder_model.onnx`,
//! `tokenizer.json`), downloaded or imported by the app. With `GRIDCRAFT_PIX2TEXT_LINES=<data
//! dir>` the test also recognises every line in `<data dir>/inputs.bin` (little-endian f32,
//! `3 * 384 * 384` values per formula, CHW, each `(pixel / 255 - 0.5) / 0.5`) and prints the
//! LaTeX. Without it, a blank line proves the graphs load, encode, decode and detokenise.

use std::path::PathBuf;

use gridcraft_ink::Recognizer;

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
}

#[test]
#[ignore = "needs the downloaded math model (see docs/ink-to-math.md)"]
fn recognises_a_formula_as_latex() {
    let Some(model_root) = env_path("GRIDCRAFT_PIX2TEXT_MODEL") else {
        eprintln!("set GRIDCRAFT_PIX2TEXT_MODEL (models root)");
        return;
    };
    let recognizer = gridcraft_ink_rten::RtenRecognizer::load_math(&model_root).expect("the model loads and verifies");
    assert_eq!(recognizer.model_id(), "pix2text-mfr");
    assert_eq!(recognizer.output(), gridcraft_ink::RecognitionOutput::Math);

    let Some(data) = env_path("GRIDCRAFT_PIX2TEXT_LINES") else {
        // No fixtures: a blank image still exercises both graphs and the tokenizer. The answer
        // is whatever the model makes of paper; what matters here is that it is a `String` and
        // nothing panicked.
        let input = vec![1.0f32; 3 * 384 * 384];
        let blank = recognizer.recognize_input(&input).expect("a blank line decodes");
        println!("blank line -> {blank:?}");
        return;
    };

    let raw = std::fs::read(data.join("inputs.bin")).expect("inputs.bin");
    let truth = std::fs::read_to_string(data.join("truth.txt")).ok();
    let mut values = Vec::with_capacity(raw.len() / 4);
    for chunk in raw.as_chunks::<4>().0 {
        values.push(f32::from_le_bytes(*chunk));
    }
    let per_line = 3 * 384 * 384;
    let lines = values.len() / per_line;
    assert!(lines > 0, "no lines to check");
    let truth: Vec<&str> = truth.as_deref().map(|t| t.lines().collect()).unwrap_or_default();
    if !truth.is_empty() {
        assert_eq!(lines, truth.len(), "inputs.bin and truth.txt disagree on the number of lines");
    }

    // Native-only test (never built for wasm), so the wall clock is available here.
    #[allow(clippy::disallowed_methods)]
    let start = std::time::Instant::now();
    let mut exact = 0usize;
    for i in 0..lines {
        let recognised = recognizer.recognize_input(&values[i * per_line..(i + 1) * per_line]).expect("recognised");
        println!("{i:3} -> {recognised:?}");
        assert!(!recognised.is_empty(), "line {i} recognised as nothing");
        if truth.get(i).is_some_and(|t| t.trim() == recognised) {
            exact += 1;
        }
    }
    let ms = start.elapsed().as_secs_f64() * 1000.0 / lines as f64;
    println!("{lines} formulas, {exact}/{lines} exact, {ms:.0} ms/formula");
}
