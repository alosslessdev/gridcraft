# Ink to Math: handwriting to an equation

> **Last reviewed:** 2026-11-07 · **Change:** created with the feature (#1778) · **Target:** Microsoft Excel (Microsoft 365) · Part of [target-app-parity.md](target-app-parity.md) and [hardware-parity.md](hardware-parity.md)

Draw › Convert › Ink to Math turns handwritten ink into an equation. The person draws a formula
with the pen, selects its strokes, and runs **Ink to Math**: the strokes are recognised as LaTeX
and replaced by an equation shape at the ink's anchor, laid out by the equation path — the same
shape Insert › Equation makes, saved as an equation and as Office Math in `.xlsx`. It is **one undo
step**, and the consumed ink comes back with that undo.

It reuses the Action Pen's plumbing ([action-pen.md](action-pen.md)): the same stroke capture in
the grid, the same `gridcraft-ink` rasteriser, the same `Recognizer` seam (`RecognitionOutput::Math`
distinguishes the math model), the same model directory and the same model panel. Only the model
and the command differ.

## The model

| | |
|---|---|
| Model | `breezedeus/pix2text-mfr` (Pix2Text MFR, Mathematical Formula Recognition) |
| Revision | `bea257edb2653f2ae413b084f2ac0e8299d08df0` |
| Licence | MIT (model card) |
| Source | <https://huggingface.co/breezedeus/pix2text-mfr> |
| Runtime | `rten` 0.27 (pure Rust, CPU), the runtime the Action Pen already uses |
| Total | 117,651,088 bytes (about 118 MB) in 3 files |

| File (inside the model directory) | Bytes | SHA-256 |
|---|---|---|
| `encoder_model.onnx` | 87,496,990 | `bd8d5c322792e9ec45793af5569e9748f82a3d728a9e00213dbfc56c1486f37d` |
| `decoder_model.onnx` | 30,114,937 | `fd0f92d7a012f3dae41e1ac79421aea0ea888b5a66cb3f9a004e424f82f3daed` |
| `tokenizer.json` | 39,161 | `3e2ab757277d22639bec28c9d7972e352d3d1dba223051fa674002dc5ab64df3` |

The graph is the same shape as the Action Pen's TrOCR: a `VisionEncoderDecoderModel` whose encoder
takes `pixel_values` (1 × 3 × 384 × 384, mean/std 0.5) and whose decoder takes `input_ids` and
`encoder_hidden_states` and answers `logits` (no key/value cache). Its own ByteLevel BPE tokenizer
(1,200 tokens) detokenises to LaTeX; the decoder numbers `<pad>` 0 and `<s>`/`</s>` 1/2, so the
loader reads the special ids from the tokenizer rather than assuming TrOCR's.

## Accuracy, measured

Windows 11 x64, CPU only, 2026-11-07. The inputs are 384 × 384 RGB renders of two formulas
(`1/2`, `x^2+1` in Cambria at 110 px), normalised as the model expects (`/255`, mean/std 0.5, CHW).

| Run | Input | Output | ms/formula |
|---|---|---|---|
| onnxruntime 1.31 pre-flight | `1/2` | `1 / 2` | — |
| `crates/ink-rten/tests/real_math_model.rs` (rten 0.27, release) | `1/2` | `1 / 2` | ~2.8 s |
| same | `x^2+1` | `\mathbf { x } ^ { \wedge } 2 + \mathbf { 1 }` | ~2.8 s |

The second row proves the runtime, preprocessing and tokenizer reproduce the reference pipeline:
both graphs load, the encoder runs, greedy decoding stops at the end token and the ByteLevel BPE
tokenizer answers LaTeX. The 2.8 s was measured while other builds shared the machine, and the
second answer shows the model's own style (bold letters) and one misread accent, which is accuracy,
not plumbing.

**Limitation, stated plainly.** These are rendered fonts, not the App's input — ink rasterised from
stroke points, at the pen's width, with no paper texture or slant. Accuracy on real handwriting,
on several formulas in one selection and on non-Latin scripts is not measured. The check above is
the way to measure it, and no weights and no fixture are committed.

No model file is committed, in `assets/` or anywhere else. The repository holds the manifest (id,
revision, paths, sizes, hashes, licence, URLs) and nothing of the weights.

## Where the model goes

The same model root and the same discovery order as the Action Pen ([action-pen.md](action-pen.md)):

```
<model root>/pix2text-mfr/encoder_model.onnx
<model root>/pix2text-mfr/decoder_model.onnx
<model root>/pix2text-mfr/tokenizer.json
```

`<model root>` is `$GRIDCRAFT_MODELS`, else the app's settings folder plus `models/`, else
`models/` beside the executable, else `models/` in the source checkout. A download goes where the
Action Pen's download goes.

## Commands

| Command | Kind | What it does |
|---|---|---|
| `draw.inkToMath` | edit (one undo step) | `{id?｜ids?: [id], points?: [[x,y],…] or [[[x,y],…],…]}` recognises the named ink (the selection; without either, the last ink stroke, like `draw.inkToShape`) or the supplied strokes, and replaces the named ink with an equation. Returns `{equation, latex}`. |
| `ink.mathStatus` | query | The math model's `filesPresent`, `ready`, id, name, revision, licence, source, size, runtime, folder, missing files, a one-line hint, and the accelerator fields (identically to `ink.status`). |
| `ink.modelImport` | native | `{path, model?: "math"}` copies and verifies the math model's files; `{model?: "handwriting"}` (the default) keeps the Action Pen model's behaviour. |
| `ink.modelRemove` | native | `{model?: "math"}` deletes the math model's folder and detaches its recognizer. |

Without the math model the command is refused with the same structured, actionable error the
Action Pen uses, naming the folder to fill:

> The math model is not installed, so Ink to Math is unavailable. Download it from Draw › Convert ›
> Ink to Math, or copy its files into `<models>/pix2text-mfr`.

`Session::ink_math` holds the recognizer (`Arc<dyn gridcraft_ink::Recognizer>` whose
`output()` is `RecognitionOutput::Math`); it is separate from `Session::ink`, the Action Pen's, so
either, both or neither model may be installed. The engine never loads a model: the desktop host
does, on a worker thread, after an explicit download or import.

## How the ink becomes an equation

1. **Collect the strokes.** The named ink shapes' points are moved to absolute sheet points (each
   shape's anchor plus its stored offsets), so several strokes — a fraction bar and its numerator,
   an `=` written in two strokes — keep their relative positions. Without an id, the last ink
   stroke on the sheet is used, as Ink to Shape does. `points` skips ink entirely.
2. **Rasterise.** `gridcraft_ink::raster::rasterize_strokes` draws them into one grey line image
   (paper white, ink black, the pen's width, paper padding, bounded segments and work), exactly as
   the Action Pen does. The image is stretched to the model's 384 × 384 input.
3. **Recognise.** The math model greedily decodes at most 256 tokens (its manifest's cap; a
   formula needs more than a line of words). A line that reaches the cap without a stop token is an
   error, never truncated LaTeX.
4. **Lay out and insert.** The LaTeX is laid out *before* anything changes: a model answer the
   layout engine refuses leaves the ink untouched. The equation is then built through
   `cmd::equation::build_equation` (11 pt, the ink's colour) at the ink's anchor, the consumed ink
   is removed and the shape pushed — all in one `edit`, so it is one undo step.
5. **Select it.** The ribbon selects the new equation after a successful conversion.

Everything is bounded: at most 32 strokes, 20,000 points per stroke (across all of them in the
rasteriser), the existing hostile-stroke rasteriser limits, and the equation source cap
(`gridcraft_math::MAX_SOURCE_BYTES`).

## The UI

Draw › Convert shows **Ink to Shape**, then **Ink to Math**. Ink to Math is enabled only while a
single ink stroke is selected (the ink must be selected for the command to know what to convert);
with the math model absent it opens the model panel, which now has a section per model (`Action
Pen` and `Ink to Math`) with its own status and Download, Import… and Remove. The equation is
painted by the normal equation painter, and the preview dialog, resizing and editing work as for any
equation.

## Privacy and safety

The same rules as the Action Pen: no network call at startup, on file open or while the panel is
open; a download only after an explicit click, over HTTPS, checked by size and SHA-256; inference
never touches the network; documents, stroke points and recognised LaTeX never leave the process.
Model files are untrusted input: paths that escape the model directory are refused, sizes are
checked before parsing and files are hashed before use.

## Reproducing the measurement

```text
GRIDCRAFT_PIX2TEXT_MODEL=<models root> \
GRIDCRAFT_PIX2TEXT_LINES=<data dir> \
  cargo test --release -p gridcraft-ink-rten --test real_math_model -- --ignored --nocapture
```

`<data dir>` holds `inputs.bin` (little-endian f32, `3 * 384 * 384` values per formula, CHW, each
`(pixel / 255 - 0.5) / 0.5`) and optionally `truth.txt` (one LaTeX source per input). Every line's
recognition is printed and must be non-empty; with `truth.txt` the exact matches are counted. Without
`GRIDCRAFT_PIX2TEXT_LINES` the test runs one blank line through both graphs and the tokenizer.

## Tests

`crates/engine/src/tests.rs` pins the behaviour without any model, with a fixed recognizer that
answers `\frac{1}{2}`: the equation shape carries that LaTeX at the ink's anchor and no ink is left;
one undo step restores both strokes and removes the equation; no model (or a text recognizer in the
math slot) keeps the workbook unchanged with the structured error; supplied strokes consume no ink;
a wrong-LaTeX answer leaves the ink alone; hostile point shapes are refusals, never panics; and the
equation survives a save and reopen with its layout rebuilt.
