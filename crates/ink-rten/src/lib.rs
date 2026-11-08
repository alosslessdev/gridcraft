//! GridCraft ink inference on `rten`: TrOCR small (handwritten) for the Action Pen, and Pix2Text
//! MFR for Draw › Convert › Ink to Math.
//!
//! Both models are standard encoder/decoder exports with the same graph shape — encoder
//! `pixel_values` → `last_hidden_state`; decoder `input_ids` + `encoder_hidden_states` →
//! `logits`, no key/value cache — a 384 × 384 RGB input normalised with mean/std 0.5 and their
//! own tokenizer. [`RtenRecognizer::load`] serves the Action Pen (`Xenova/trocr-small-handwritten`,
//! MIT) and [`RtenRecognizer::load_math`] Ink to Math (`breezedeus/pix2text-mfr`, MIT); each
//! verifies its files against the matching [`gridcraft_ink::Manifest`] and reads them from the
//! model directory the person downloaded into or filled by hand (see `docs/action-pen.md` and
//! `docs/ink-to-math.md`).
//!
//! This crate is native only (`crates/ink-rten` sits at layer 6, so `cargo xtask wasm` skips it):
//! `rten` reads model files with the standard library and is not built for the web app.
//!
//! Inference is greedy decoding, one line at a time: the tokenizer's start token, stop at its end
//! token. A line that reaches the manifest's step cap without one does not fit the model's input
//! and is a [`Error::Recognize`], never truncated text. On this machine one line of handwriting
//! takes about 0.27 s (CPU, release).
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
#![forbid(unsafe_code)]

use std::path::Path;

use gridcraft_ink::{Error, IMAGE_SIZE, LineImage, Manifest, PIX2TEXT_MFR, RecognitionOutput, Recognizer, TROCR_SMALL_HANDWRITTEN};
use tokenizers::Tokenizer;

/// TrOCR's decoder start token (`</s>`, id 2 in that export); used when a tokenizer names none.
pub const START: i32 = 2;
/// End of the line (`</s>` in both exports).
pub const EOS: i32 = 2;
/// TrOCR's padding token (`<pad>`, id 1 in that export); used when a tokenizer names none.
pub const PAD: i32 = 1;
/// Longest line the handwriting model decodes, in tokens, including the stop token's step.
pub const MAX_STEPS: usize = TROCR_SMALL_HANDWRITTEN.max_steps;

/// The recognizer, loaded from a model directory.
pub struct RtenRecognizer {
    encoder: rten::Model,
    decoder: rten::Model,
    ids_node: rten::NodeId,
    encoder_node: rten::NodeId,
    logits_node: rten::NodeId,
    /// The model's own tokenizer: its `decode` is the reference detokenisation (SentencePiece
    /// unigram for TrOCR, ByteLevel BPE for Pix2Text MFR), so ids turn into text exactly as the
    /// model card's pipeline does.
    tokenizer: Tokenizer,
    id: &'static str,
    /// The decoder's start, stop and padding ids, read from the tokenizer so each export's own
    /// vocabulary governs (the two models number `<pad>` differently).
    start: i32,
    eos: i32,
    pad: i32,
    /// The manifest's step cap: a longer answer is refused, never truncated.
    max_steps: usize,
    output: RecognitionOutput,
}

/// Greedy decoding: asks `next` for a token until it emits `eos` (or `pad`, which a decode step
/// should not emit), appending each token to `ids` and returning what it produced.
///
/// Reaching `max_steps` without a stop token means the writing does not fit one line. The tokens
/// so far are a prefix of the real answer, so returning them would insert a truncated value:
/// this is an error instead.
fn greedy(mut next: impl FnMut(&[i32]) -> Result<i32, Error>, ids: &mut Vec<i32>, max_steps: usize, eos: i32, pad: i32) -> Result<Vec<i32>, Error> {
    let mut produced = Vec::new();
    for _ in 0..max_steps {
        let token = next(ids)?;
        if token == eos || token == pad {
            return Ok(produced);
        }
        ids.push(token);
        produced.push(token);
    }
    Err(Error::Recognize(format!(
        "The writing is too long to recognise in one entry: the model reached its limit of {max_steps} tokens. Write less at a time."
    )))
}

impl RtenRecognizer {
    /// Loads the shipped TrOCR small model from `root` (the models root; the model itself is
    /// `<root>/trocr-small-handwritten`).
    pub fn load(root: &Path) -> Result<Self, Error> {
        Self::from_manifest(root, &TROCR_SMALL_HANDWRITTEN)
    }

    /// Loads the Ink to Math model from `root` (`<root>/pix2text-mfr`).
    pub fn load_math(root: &Path) -> Result<Self, Error> {
        Self::from_manifest(root, &PIX2TEXT_MFR)
    }

    /// Loads the model `manifest` describes from `root`, verifying every file's size and SHA-256
    /// first. A corrupt or incomplete copy is an error, never a partial load.
    pub fn from_manifest(root: &Path, manifest: &Manifest) -> Result<Self, Error> {
        manifest.verify(root)?;
        let dir = manifest.dir(root);
        let mut files = manifest.files.iter();
        let encoder_file = files.next().ok_or_else(|| Error::Corrupt("empty manifest".into()))?;
        let decoder_file = files.next().ok_or_else(|| Error::Corrupt("empty manifest".into()))?;
        let tokenizer_file = files.next().ok_or_else(|| Error::Corrupt("empty manifest".into()))?;
        let encoder =
            rten::Model::load_file(dir.join(encoder_file.path)).map_err(|e| Error::Io(format!("{}: {e}", dir.join(encoder_file.path).display())))?;
        let decoder =
            rten::Model::load_file(dir.join(decoder_file.path)).map_err(|e| Error::Io(format!("{}: {e}", dir.join(decoder_file.path).display())))?;
        let ids_node = decoder.node_id("input_ids").map_err(|e| Error::Corrupt(format!("decoder has no input_ids: {e}")))?;
        let encoder_node =
            decoder.node_id("encoder_hidden_states").map_err(|e| Error::Corrupt(format!("decoder has no encoder_hidden_states: {e}")))?;
        let logits_node = decoder.node_id("logits").map_err(|e| Error::Corrupt(format!("decoder has no logits: {e}")))?;
        let tokenizer = Tokenizer::from_file(dir.join(tokenizer_file.path))
            .map_err(|e| Error::Corrupt(format!("{}: {e}", dir.join(tokenizer_file.path).display())))?;
        if tokenizer.get_vocab_size(true) < 4 {
            return Err(Error::Corrupt("the tokenizer has no vocabulary".into()));
        }
        // Each export numbers its special tokens itself: TrOCR's `<pad>` is 1, Pix2Text MFR's is
        // 0. Read them from the tokenizer the model ships with, and fall back only when an
        // export leaves them unnamed.
        let token_id = |name: &str, fallback: i32| tokenizer.token_to_id(name).and_then(|i| i32::try_from(i).ok()).unwrap_or(fallback);
        let (start, eos, pad) = (token_id("</s>", START), token_id("</s>", EOS), token_id("<pad>", PAD));
        Ok(RtenRecognizer {
            encoder,
            decoder,
            ids_node,
            encoder_node,
            logits_node,
            tokenizer,
            id: manifest.id,
            start,
            eos,
            pad,
            max_steps: manifest.max_steps,
            output: manifest.output,
        })
    }

    /// One decode step's token: the highest-scoring id at the last position.
    fn next_token(&self, ids: &[i32], encoder: &rten::Value) -> Result<i32, Error> {
        let len = ids.len().clamp(1, self.max_steps + 1);
        let input_ids = rten::Value::from_shape([1usize, len], ids.to_vec()).map_err(|e| Error::Recognize(format!("input ids: {e}")))?;
        let outs = self
            .decoder
            .run(vec![(self.ids_node, rten::ValueOrView::Value(input_ids)), (self.encoder_node, encoder.as_view().into())], &[self.logits_node], None)
            .map_err(|e| Error::Recognize(format!("decoder: {e}")))?;
        let logits = outs.into_iter().next().ok_or_else(|| Error::Recognize("decoder returned nothing".into()))?;
        argmax_last(&logits)
    }
}

/// Highest-scoring id at the last position of `[1, sequence, vocabulary]` logits.
fn argmax_last(logits: &rten::Value) -> Result<i32, Error> {
    use rten_tensor::layout::Layout;
    let rten::ValueView::FloatTensor(t) = logits.as_view() else {
        return Err(Error::Recognize("the decoder's logits are not a float tensor".into()));
    };
    let shape = t.shape();
    if shape.len() != 3 || shape[0] != 1 || shape[1] == 0 {
        return Err(Error::Recognize(format!("unexpected logits shape {shape:?}")));
    }
    let (last, vocab) = (shape[1] - 1, shape[2]);
    let mut best = 0usize;
    let mut best_score = f32::NEG_INFINITY;
    for v in 0..vocab {
        let score = t[[0, last, v]];
        if score > best_score {
            best_score = score;
            best = v;
        }
    }
    i32::try_from(best).map_err(|_| Error::Recognize("token id out of range".into()))
}

impl RtenRecognizer {
    /// Text for the tokens a decode step produced, through the model's tokenizer.
    fn decode(&self, ids: &[i32]) -> Result<String, Error> {
        let ids: Vec<u32> = ids.iter().filter_map(|i| u32::try_from(*i).ok()).collect();
        let text = self.tokenizer.decode(&ids, true).map_err(|e| Error::Recognize(format!("tokenizer: {e}")))?;
        Ok(text.trim().to_string())
    }

    /// Recognises one preprocessed line: `3 * 384 * 384` values in CHW order, each
    /// `(pixel / 255 - 0.5) / 0.5`. [`Recognizer::recognize`] builds this from rasterised ink;
    /// the real-model test feeds it a photograph's input, so the runtime and tokenizer are
    /// measured on the distribution the model was trained for.
    pub fn recognize_input(&self, input: &[f32]) -> Result<String, Error> {
        if input.len() != 3 * IMAGE_SIZE * IMAGE_SIZE {
            return Err(Error::Recognize(format!("expected {} input values, got {}", 3 * IMAGE_SIZE * IMAGE_SIZE, input.len())));
        }
        let tensor = rten::Value::from_shape([1usize, 3, IMAGE_SIZE, IMAGE_SIZE], input.to_vec())
            .map_err(|e| Error::Recognize(format!("input image: {e}")))?;
        let encoder = self.encoder.run_one(rten::ValueOrView::Value(tensor), None).map_err(|e| Error::Recognize(format!("encoder: {e}")))?;
        let mut ids: Vec<i32> = vec![self.start];
        let produced = greedy(|ids| self.next_token(ids, &encoder), &mut ids, self.max_steps, self.eos, self.pad)?;
        self.decode(&produced)
    }
}

impl Recognizer for RtenRecognizer {
    fn recognize(&self, image: &LineImage) -> Result<String, Error> {
        if image.width == 0 || image.height == 0 || image.pixels.len() != image.width * image.height {
            return Err(Error::Ink("There is no ink to recognise.".into()));
        }
        self.recognize_input(&gridcraft_ink::raster::model_input(image))
    }

    fn model_id(&self) -> &'static str {
        self.id
    }

    fn output(&self) -> RecognitionOutput {
        self.output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_that_hits_the_step_limit_is_an_error_not_truncated_text() {
        // A model that never emits a stop token: the line does not fit, so decoding fails.
        let mut ids = vec![START];
        let endless = greedy(|_| Ok(7), &mut ids, MAX_STEPS, EOS, PAD);
        assert!(matches!(endless, Err(Error::Recognize(_))), "{endless:?}");
        assert_eq!(ids.len(), 1 + MAX_STEPS, "every token was consumed before failing");

        // A stop token after two words ends the decode normally, with just those tokens.
        let mut ids = vec![START];
        let mut asked = 0;
        let produced = greedy(
            |_| {
                asked += 1;
                Ok(if asked == 3 { EOS } else { 7 })
            },
            &mut ids,
            MAX_STEPS,
            EOS,
            PAD,
        )
        .expect("the stop token ends the decode");
        assert_eq!(produced, vec![7, 7]);
        assert_eq!(ids, vec![START, 7, 7]);

        // A formula's cap is its own manifest's, and the pads differ between the exports: the
        // Pix2Text tokenizer numbers `<pad>` 0, TrOCR's 1.
        let mut ids = vec![START];
        let endless = greedy(|_| Ok(7), &mut ids, PIX2TEXT_MFR.max_steps, EOS, 0);
        assert!(matches!(endless, Err(Error::Recognize(_))), "{endless:?}");
        assert_eq!(ids.len(), 1 + PIX2TEXT_MFR.max_steps);
        let mut ids = vec![START];
        assert_eq!(greedy(|_| Ok(0), &mut ids, PIX2TEXT_MFR.max_steps, EOS, 0).expect("pad ends the decode"), Vec::<i32>::new());
    }

    /// The model directory when this machine has one (never committed, so CI skips these
    /// checks). Looks where the app would.
    fn installed_dir() -> Option<std::path::PathBuf> {
        gridcraft_ink::roots()
            .into_iter()
            .map(|root| gridcraft_ink::model_dir(&root, TROCR_SMALL_HANDWRITTEN.id))
            .find(|dir| dir.join("tokenizer.json").is_file())
    }

    #[test]
    fn the_model_tokenizer_loads_and_decodes_like_the_reference() {
        // Reads the model directory when a developer has it (never in CI, and never committed).
        let Some(dir) = installed_dir() else { return };
        let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).expect("tokenizer.json");
        assert!(tokenizer.get_vocab_size(true) > 1000);
        // Ids 4 and 5 are "▁the" and "." in this vocabulary: the metaspace marker becomes a space.
        assert_eq!(tokenizer.decode(&[4, 5], true).expect("decode"), " the.");
        assert_eq!(tokenizer.decode(&[4, 5], true).expect("decode").trim(), "the.");
    }
}
