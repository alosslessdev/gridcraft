//! The model manifest: which files a model is made of, where they come from and how to check
//! them. No weights are committed or shipped; only this description is (see
//! `docs/action-pen.md` and `MODEL-POLICY` in `docs/decisions.md`).

use std::path::{Path, PathBuf};

use sha2::Digest;

use crate::{Accelerator, Error, RecognitionOutput, is_safe_relative_path};

/// One file of a model, relative to the model's directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelFile {
    /// Path inside the model directory, e.g. `onnx/encoder_model.onnx`.
    pub path: &'static str,
    /// Exact size in bytes.
    pub bytes: u64,
    /// Lower-case hex SHA-256 of the file.
    pub sha256: &'static str,
    /// Where to download it from (HTTPS).
    pub url: &'static str,
}

/// A model GridCraft can use: its provenance, licence and files.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Manifest {
    /// Directory name under the model root, also the id commands take.
    pub id: &'static str,
    /// Human-readable name for the UI.
    pub name: &'static str,
    /// The upstream revision the hashes were taken from.
    pub revision: &'static str,
    /// The licence the model card states (not the runtime's).
    pub licence: &'static str,
    /// Where the model comes from, as shown in the download prompt.
    pub source: &'static str,
    /// The runtime and version the manifest's hashes and the measured accuracy belong to.
    pub runtime: &'static str,
    /// What the model answers — typed text for a cell, or LaTeX for an equation — so the engine
    /// never writes one into the other's place.
    pub output: RecognitionOutput,
    /// Longest answer, in decoder tokens, including the stop token's step. A line that reaches
    /// it without a stop token is refused rather than returned truncated. Bounded by the
    /// decoder's 512 learned positions.
    pub max_steps: usize,
    /// The accelerators the model can run on, best first. CPU is always there; an optional
    /// backend crate (`docs/action-pen.md`) would add one, never replace it.
    pub accelerators: &'static [Accelerator],
    pub files: &'static [ModelFile],
}

/// TrOCR small (handwritten), fp32 ONNX exported by Xenova. The quantized exports are not used:
/// rten's quantized path disagrees with onnxruntime on this model (see `docs/decisions.md`).
pub const TROCR_SMALL_HANDWRITTEN: Manifest = Manifest {
    id: "trocr-small-handwritten",
    name: "TrOCR small (handwritten)",
    revision: "02bea33ca09a70e2e2d91140a7e6e1a729606a5b",
    licence: "MIT",
    source: "https://huggingface.co/Xenova/trocr-small-handwritten",
    runtime: "rten 0.27 (pure Rust, CPU)",
    output: RecognitionOutput::Text,
    max_steps: 32,
    accelerators: &[Accelerator::Cpu],
    files: &[
        ModelFile {
            path: "onnx/encoder_model.onnx",
            bytes: 87_496_990,
            sha256: "f6a2c3ac23a426c85f8557f2763bf39cce7eeacf3004563762e023050ddc1fd9",
            url: "https://huggingface.co/Xenova/trocr-small-handwritten/resolve/02bea33ca09a70e2e2d91140a7e6e1a729606a5b/onnx/encoder_model.onnx",
        },
        ModelFile {
            path: "onnx/decoder_model.onnx",
            bytes: 158_814_912,
            sha256: "4cf40055fbca767b2fd34f1de4f86be41d8dac670028bb6f65a3d35ebe0b0e5f",
            url: "https://huggingface.co/Xenova/trocr-small-handwritten/resolve/02bea33ca09a70e2e2d91140a7e6e1a729606a5b/onnx/decoder_model.onnx",
        },
        ModelFile {
            path: "tokenizer.json",
            bytes: 4_494_727,
            sha256: "68bcb5468c854362a615f3d2ff6a5e4091a85f4c8198993ed9a30afe0b143737",
            url: "https://huggingface.co/Xenova/trocr-small-handwritten/resolve/02bea33ca09a70e2e2d91140a7e6e1a729606a5b/tokenizer.json",
        },
    ],
};

/// Pix2Text MFR (Mathematical Formula Recognition), MIT: a TrOCR vision-encoder/decoder
/// fine-tuned on handwritten and printed formulas, exported to ONNX with the same graph shape as
/// [`TROCR_SMALL_HANDWRITTEN`] (encoder `pixel_values` → `last_hidden_state`; decoder
/// `input_ids` + `encoder_hidden_states` → `logits`; 384 × 384 RGB, mean/std 0.5; ByteLevel BPE
/// tokenizer over LaTeX, 1,200 tokens). It answers a formula's LaTeX source, which Draw ›
/// Convert › Ink to Math inserts as an equation (`docs/ink-to-math.md`).
pub const PIX2TEXT_MFR: Manifest = Manifest {
    id: "pix2text-mfr",
    name: "Pix2Text MFR (handwritten maths)",
    revision: "bea257edb2653f2ae413b084f2ac0e8299d08df0",
    licence: "MIT",
    source: "https://huggingface.co/breezedeus/pix2text-mfr",
    runtime: "rten 0.27 (pure Rust, CPU)",
    output: RecognitionOutput::Math,
    // Formulas are longer than a line of words; the decoder has 512 learned positions.
    max_steps: 256,
    accelerators: &[Accelerator::Cpu],
    files: &[
        ModelFile {
            path: "encoder_model.onnx",
            bytes: 87_496_990,
            sha256: "bd8d5c322792e9ec45793af5569e9748f82a3d728a9e00213dbfc56c1486f37d",
            url: "https://huggingface.co/breezedeus/pix2text-mfr/resolve/bea257edb2653f2ae413b084f2ac0e8299d08df0/encoder_model.onnx",
        },
        ModelFile {
            path: "decoder_model.onnx",
            bytes: 30_114_937,
            sha256: "fd0f92d7a012f3dae41e1ac79421aea0ea888b5a66cb3f9a004e424f82f3daed",
            url: "https://huggingface.co/breezedeus/pix2text-mfr/resolve/bea257edb2653f2ae413b084f2ac0e8299d08df0/decoder_model.onnx",
        },
        ModelFile {
            path: "tokenizer.json",
            bytes: 39_161,
            sha256: "3e2ab757277d22639bec28c9d7972e352d3d1dba223051fa674002dc5ab64df3",
            url: "https://huggingface.co/breezedeus/pix2text-mfr/resolve/bea257edb2653f2ae413b084f2ac0e8299d08df0/tokenizer.json",
        },
    ],
};

/// How complete a model's copy in one directory is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallState {
    /// Every file is there with its exact size (contents are checked by [`Manifest::verify`]).
    Installed,
    /// Files that are missing or have the wrong size.
    Incomplete(Vec<String>),
    /// A file that says why it can't be used.
    Refused(String),
}

impl InstallState {
    pub fn is_installed(&self) -> bool {
        matches!(self, InstallState::Installed)
    }
}

impl Manifest {
    /// Total download size, in bytes.
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().map(|f| f.bytes).sum()
    }

    /// The manifest entry whose file name is `name` (`encoder_model.onnx`, also accepted with
    /// its path). Used by manual import, where a person picks the downloaded files one by one.
    pub fn file_named(&self, name: &str) -> Option<&ModelFile> {
        self.files.iter().find(|f| f.path == name || f.path.rsplit('/').next() == Some(name))
    }

    /// The model's directory under `root`.
    pub fn dir(&self, root: &Path) -> PathBuf {
        crate::model_dir(root, self.id)
    }

    /// Refuses a manifest that could escape its model directory or whose hashes make no sense.
    /// The shipped manifest passes; this guards a future one edit away from a traversal.
    pub fn validate(&self) -> Result<(), Error> {
        if self.files.is_empty() {
            return Err(Error::Corrupt(format!("{} lists no files", self.id)));
        }
        if !self.accelerators.contains(&Accelerator::Cpu) {
            return Err(Error::Corrupt(format!("{} must run on CPU: that is the backend every platform has", self.id)));
        }
        // The decoder's learned positions: a step cap outside them would decode past the model.
        if self.max_steps == 0 || self.max_steps > 512 {
            return Err(Error::Corrupt(format!("{} has an unusable step cap of {} (1 to 512)", self.id, self.max_steps)));
        }
        for f in self.files {
            if !is_safe_relative_path(f.path) {
                return Err(Error::Corrupt(format!("{} has an unsafe file path {:?}", self.id, f.path)));
            }
            if f.bytes == 0 {
                return Err(Error::Corrupt(format!("{} lists {} as empty", self.id, f.path)));
            }
            if !is_sha256_hex(f.sha256) {
                return Err(Error::Corrupt(format!("{} has no valid SHA-256 for {}", self.id, f.path)));
            }
            if !f.url.starts_with("https://") {
                return Err(Error::Corrupt(format!("{} downloads {} over something other than HTTPS", self.id, f.path)));
            }
        }
        Ok(())
    }

    /// The cheap check: every file is there and has its exact size. One `stat` per file, so the
    /// UI may run it while its panel is open. [`Manifest::verify`] reads the contents.
    pub fn check_sizes(&self, root: &Path) -> InstallState {
        let dir = self.dir(root);
        let mut bad = Vec::new();
        for f in self.files {
            match std::fs::metadata(dir.join(f.path)) {
                Ok(m) if m.is_file() && m.len() == f.bytes => {}
                Ok(m) if m.is_file() => bad.push(format!("{} ({} of {} bytes)", f.path, m.len(), f.bytes)),
                _ => bad.push(f.path.to_string()),
            }
        }
        if bad.is_empty() { InstallState::Installed } else { InstallState::Incomplete(bad) }
    }

    /// Full verification: sizes and SHA-256 of every file. Streams each file, so a 250 MB model
    /// is never held in memory.
    pub fn verify(&self, root: &Path) -> Result<(), Error> {
        self.validate()?;
        let dir = self.dir(root);
        for f in self.files {
            let path = dir.join(f.path);
            let meta = std::fs::metadata(&path).map_err(|e| Error::Incomplete(format!("missing {} ({e})", f.path)))?;
            if !meta.is_file() {
                return Err(Error::Incomplete(format!("{} is not a file", f.path)));
            }
            if meta.len() != f.bytes {
                return Err(Error::Incomplete(format!("{} is {} bytes, expected {}", f.path, meta.len(), f.bytes)));
            }
            let got = sha256_file(&path)?;
            if !got.eq_ignore_ascii_case(f.sha256) {
                return Err(Error::Corrupt(format!("{} does not match its SHA-256 (expected {}, got {got}); download it again", f.path, f.sha256)));
            }
        }
        Ok(())
    }
}

/// Lower-case hex SHA-256 of a whole file, streamed in 64 KiB chunks.
pub fn sha256_file(path: &Path) -> Result<String, Error> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
    let mut hasher = sha2::Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| Error::Io(format!("{}: {e}", path.display())))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex(&hasher.finalize()))
}

/// Hex SHA-256 of `bytes`.
pub fn sha256_bytes(bytes: &[u8]) -> String {
    hex(&sha2::Sha256::digest(bytes))
}

fn hex(digest: &[u8]) -> String {
    let mut s = String::with_capacity(digest.len() * 2);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn is_sha256_hex(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A scratch directory that removes itself.
    struct Scratch(PathBuf);

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn scratch(tag: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("gridcraft-ink-test-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        if std::fs::create_dir_all(&dir).is_err() {
            // Tests may not unwrap; a failed scratch directory fails the assertions below.
            return Scratch(dir);
        }
        Scratch(dir)
    }

    const ONE: Manifest = Manifest {
        id: "tiny",
        name: "Tiny",
        revision: "0",
        licence: "MIT",
        source: "https://example.invalid/tiny",
        runtime: "rten 0.27 (pure Rust, CPU)",
        output: RecognitionOutput::Text,
        max_steps: 32,
        accelerators: &[Accelerator::Cpu],
        files: &[ModelFile {
            path: "m.onnx",
            bytes: 3,
            sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            url: "https://example.invalid/m.onnx",
        }],
    };

    #[test]
    fn shipped_manifest_is_valid_and_has_the_expected_size() {
        assert_eq!(TROCR_SMALL_HANDWRITTEN.validate(), Ok(()));
        assert_eq!(TROCR_SMALL_HANDWRITTEN.total_bytes(), 87_496_990 + 158_814_912 + 4_494_727);
        assert_eq!(TROCR_SMALL_HANDWRITTEN.files.len(), 3);
        assert!(TROCR_SMALL_HANDWRITTEN.accelerators.contains(&Accelerator::Cpu));
        assert!(!TROCR_SMALL_HANDWRITTEN.runtime.is_empty());
        assert!(TROCR_SMALL_HANDWRITTEN.file_named("tokenizer.json").is_some());
        assert!(TROCR_SMALL_HANDWRITTEN.file_named("onnx/decoder_model.onnx").is_some());
        assert!(TROCR_SMALL_HANDWRITTEN.file_named("decoder_model.onnx").is_some());
    }

    #[test]
    fn the_math_manifest_is_valid_and_answers_latex() {
        assert_eq!(PIX2TEXT_MFR.validate(), Ok(()));
        assert_eq!(PIX2TEXT_MFR.output, RecognitionOutput::Math);
        assert_eq!(TROCR_SMALL_HANDWRITTEN.output, RecognitionOutput::Text);
        assert_eq!(PIX2TEXT_MFR.total_bytes(), 87_496_990 + 30_114_937 + 39_161);
        // Three files in the order the loader reads them: encoder, decoder, tokenizer.
        let paths: Vec<&str> = PIX2TEXT_MFR.files.iter().map(|f| f.path).collect();
        assert_eq!(paths, ["encoder_model.onnx", "decoder_model.onnx", "tokenizer.json"]);
        let (math_steps, text_steps) = (PIX2TEXT_MFR.max_steps, TROCR_SMALL_HANDWRITTEN.max_steps);
        assert!(math_steps > text_steps, "a formula needs more tokens than a line of words");
        // The range check above (1 to 512, the decoder's learned positions) is part of `validate`.
        // The model card's licence is the model's, not the runtime's.
        assert_eq!(PIX2TEXT_MFR.licence, "MIT");
        // A step cap past the decoder's learned positions is refused.
        const TOO_LONG: Manifest = Manifest { max_steps: 513, ..PIX2TEXT_MFR };
        assert!(matches!(TOO_LONG.validate(), Err(Error::Corrupt(_))));
        const NO_STEPS: Manifest = Manifest { max_steps: 0, ..PIX2TEXT_MFR };
        assert!(matches!(NO_STEPS.validate(), Err(Error::Corrupt(_))));
    }

    #[test]
    fn accelerators_round_trip_through_their_wire_names() {
        for a in [Accelerator::Cpu, Accelerator::OpenVino, Accelerator::Apple] {
            assert_eq!(Accelerator::parse(a.as_str()), Some(a));
        }
        assert_eq!(Accelerator::parse("OpenVINO"), Some(Accelerator::OpenVino));
        assert_eq!(Accelerator::parse("npu"), Some(Accelerator::OpenVino));
        assert_eq!(Accelerator::parse("quantum"), None);
        // A manifest that drops CPU would make the feature depend on an accelerator: refused.
        const NO_CPU: Manifest = Manifest { accelerators: &[Accelerator::OpenVino], ..ONE };
        assert!(matches!(NO_CPU.validate(), Err(Error::Corrupt(_))));
    }

    #[test]
    fn traversal_paths_are_refused() {
        for p in ["../x.onnx", "a/../../x", "/etc/passwd", "C:\\x.onnx", "a\\b", "", "./x", "a//b", "http://x/y"] {
            assert!(!is_safe_relative_path(p), "{p:?} should be refused");
        }
        assert!(is_safe_relative_path("onnx/encoder_model.onnx"));
        assert!(is_safe_relative_path("tokenizer.json"));
        const BAD: Manifest = Manifest { files: &[ModelFile { path: "../escape.onnx", ..ONE.files[0] }], ..ONE };
        assert!(matches!(BAD.validate(), Err(Error::Corrupt(_))));
    }

    #[test]
    fn a_downloaded_file_is_verified_by_hash_and_size() {
        let dir = scratch("verify");
        let model = dir.0.join("tiny");
        if std::fs::create_dir_all(&model).is_err() {
            return;
        }
        // "abc" has the SHA-256 below.
        if std::fs::write(model.join("m.onnx"), b"abc").is_err() {
            return;
        }
        assert_eq!(ONE.check_sizes(&dir.0), InstallState::Installed);
        assert_eq!(ONE.verify(&dir.0), Ok(()));

        // Same size, different bytes: the hash check catches it.
        if std::fs::write(model.join("m.onnx"), b"abd").is_err() {
            return;
        }
        assert_eq!(ONE.check_sizes(&dir.0), InstallState::Installed);
        assert!(matches!(ONE.verify(&dir.0), Err(Error::Corrupt(_))));

        // Wrong size: the cheap check catches it before any reading.
        if std::fs::write(model.join("m.onnx"), b"abcd").is_err() {
            return;
        }
        assert!(matches!(ONE.check_sizes(&dir.0), InstallState::Incomplete(_)));
    }

    #[test]
    fn a_missing_file_is_reported_by_name() {
        let dir = scratch("missing");
        match ONE.check_sizes(&dir.0) {
            InstallState::Incomplete(bad) => assert_eq!(bad, vec!["m.onnx".to_string()]),
            other => panic!("unexpected state {other:?}"),
        }
    }
}
