# Gridwell test harness

A single corpus of example tables, rendered to every output format, verified two
ways:

1. **Textual snapshots** (`insta`) — the hard regression gate on *all* 11 writers.
2. **Visual gallery + image diff** — rendered PNGs you can *see*, with an
   automated perceptual diff on a small deterministic subset.

## Layout

```
crates/gridwell-testkit/   # builder DSL + the example registry (source of truth)
xtask/                     # harness tooling: corpus, gallery, image diff
harness/
  Dockerfile               # pinned renderer toolchain (chromium/xelatex/typst/libreoffice/…)
  goldens/<fmt>/<name>.png # committed golden images for the gated subset (Git LFS)
  corpus/                  # generated IR JSON            (gitignored)
  gallery/                 # generated gallery + previews (gitignored)
```

## The corpus

Every example lives in [`gridwell-testkit`](../crates/gridwell-testkit) as a
function that builds a `gridwell_ir::Table` via the fluent builder, registered in
`examples::all()` with a name, category, and feature tags. It is the single
source of truth for both the snapshot tests and this harness.

Add an example by writing a `fn` in `crates/gridwell-testkit/src/examples.rs` and
adding one `ex(...)` line to `all()`. The crate's tests assert every example is
valid, uniquely named, and round-trips through JSON.

## Textual snapshots (all formats)

Each writer's `tests/snapshots.rs` renders every example and asserts a named
snapshot (`crates/gridwell-writer-*/tests/snapshots/snapshots__<name>.snap`).
Binary writers (docx/xlsx/pptx) snapshot the unzipped XML plus a zip-validity
check.

```bash
cargo test                       # run the gate (594 snapshots)
cargo insta review               # review pending changes interactively
INSTA_UPDATE=always cargo test   # accept all changes (regenerate .snap files)
```

The committed `.snap` files are the accept-log; a `.snap.new` file marks a
changed snapshot awaiting review.

## Visual gallery (see the rendered result)

```bash
cargo xtask corpus     # write harness/corpus/<name>.json for every example
cargo xtask gallery    # render every example to every format, rasterize, build the gallery
open harness/gallery/index.html
```

The gallery is a matrix of examples × formats. Text formats (ANSI/Pandoc/Quarto)
are shown as source; everything else is rasterized to a PNG by shelling out to an
external renderer:

| Format(s)                     | Rasterizer            |
|-------------------------------|-----------------------|
| HTML                          | headless Chromium     |
| SVG                           | resvg                 |
| Typst                         | `typst compile`       |
| LaTeX                         | xelatex → pdftoppm    |
| RTF, DOCX, XLSX, PPTX         | LibreOffice → pdftoppm|

Missing renderers degrade gracefully: the cell is marked *unavailable* instead of
failing the run, so the gallery is useful even on a machine without the full
toolchain.

## Visual regression gate (gated subset)

Pixel-diffing rasterized output is only reliable for **deterministic** renderers,
so the *blocking* image gate covers just **HTML, SVG, and Typst**. LaTeX and the
LibreOffice-driven formats (font/version sensitive) appear in the gallery and the
report but never fail CI on pixels.

```bash
cargo xtask gallery --check    # diff gated formats vs harness/goldens; exit 2 on regression
cargo xtask gallery --accept   # update goldens from the current render (the visual "approve")
```

A comparison is a regression when more than 0.5% of pixels differ beyond a small
per-channel tolerance, or dimensions change. New/other-format changes are
reported but non-blocking.

### Seeding goldens

Goldens must be reproducible, so generate them **inside the pinned Docker image**,
not on an arbitrary machine:

```bash
docker build -f harness/Dockerfile -t gridwell-harness:pinned-2026-07 .
docker run --rm -v "$PWD:/w" -w /w gridwell-harness:pinned-2026-07 \
    cargo xtask gallery --accept
git lfs install
git add harness/goldens && git commit -m "Seed harness goldens"
```

## CI

- **`ci.yml`** runs `cargo test` (the textual gate) on Linux/macOS/Windows and,
  on failure, uploads pending `.snap.new` files with a review hint.
- **`harness-image.yml`** builds + pushes the pinned image to GHCR when the
  Dockerfile changes.
- **`visual.yml`** runs `cargo xtask gallery --check` inside the pinned image,
  uploads the gallery as a build artifact, and writes a per-format summary
  table. It fails only on a gated regression.

## Testing multiple renderer versions (deferred)

The Docker image tag encodes the toolchain set. `visual.yml` uses a one-entry
`strategy.matrix.toolchain`; to test another renderer version, build a new image
tag and add it to that matrix — no other changes required.
