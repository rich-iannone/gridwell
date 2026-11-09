# Pinned toolchain for the Gridwell visual test harness.
#
# This image fixes the versions of every external renderer so that the gated
# visual-diff (every rasterized format) is reproducible across machines and CI runs. The
# image tag encodes the toolchain set (e.g. gridwell-harness:pinned-2026-07); to
# test a *new* renderer version, build a new tag and add it to the CI matrix in
# .github/workflows/visual.yml — do not mutate an existing tag.
#
# Build:  docker build -f harness/Dockerfile -t gridwell-harness:pinned-2026-07 .
# Run:    docker run --rm -v "$PWD:/w" -w /w gridwell-harness:pinned-2026-07 \
#             cargo xtask gallery --check

# Pinned Rust + Debian bookworm base.
# Latest stable Rust on Debian bookworm. The Rust version is intentionally NOT
# pinned to an exact toolchain: it doesn't affect the rendered pixels (the
# renderers below are what's pinned), and floating it avoids breakage when a
# dependency raises its minimum (e.g. edition 2024 needs Rust >= 1.85).
FROM rust:1-bookworm

ENV DEBIAN_FRONTEND=noninteractive

# System renderers from Debian bookworm (a stable release, not a dated apt
# snapshot: the buildx layer cache is what keeps a built layer as it is):
#  - chromium          : HTML and SVG screenshots (gated)
#  - librsvg2-bin       : rsvg-convert, for the SVG writer's width calibration
#                         (a viewer that ignores `textLength`; not gated)
#  - poppler-utils      : pdftoppm (PDF -> PNG for LaTeX/LibreOffice)
#  - texlive-*          : xelatex + standalone.cls (LaTeX -> PDF)
#  - libreoffice        : RTF/DOCX/XLSX/PPTX -> PDF
#  - fonts-noto*        : CJK / emoji / RTL coverage for LaTeX & Typst
RUN apt-get update && apt-get install -y --no-install-recommends \
        chromium \
        librsvg2-bin \
        poppler-utils \
        texlive-latex-base \
        texlive-latex-extra \
        texlive-xetex \
        texlive-fonts-recommended \
        libreoffice-calc \
        libreoffice-writer \
        libreoffice-impress \
        fonts-noto-core \
        fonts-noto-cjk \
        fonts-noto-color-emoji \
        ca-certificates \
        git-lfs \
    && rm -rf /var/lib/apt/lists/*

# Typst isn't packaged for bookworm; install a pinned CLI via cargo.
RUN cargo install typst-cli --version 0.12.0 --locked

# Chromium in a container must run without the sandbox; the xtask already
# passes --no-sandbox, but export a hint for anything else that shells out.
ENV CHROME_BIN=/usr/bin/chromium

# Warm the font cache so text metrics are stable on first run.
RUN fc-cache -f

# `ulem` (\uline, \sout) for the LaTeX writer's underline and strike-through.
# It is in texlive-plain-generic, which texlive-latex-extra only recommends.
# Added as its own final layer so the cached layers above (and so every other
# renderer) stay exactly as they were.
RUN apt-get update && apt-get install -y --no-install-recommends texlive-plain-generic \
    && rm -rf /var/lib/apt/lists/*

# Roboto: the font the SVG writer lays text out with (crates/gridwell-writer-svg,
# measure.rs), so Chromium draws the SVG goldens in the font their widths describe,
# and rsvg (which ignores `textLength`) shows the calibration test how well they fit;
# that test requires it here (GRIDWELL_REQUIRE_ROBOTO). Its own final layer, like ulem above, so every earlier
# layer stays exactly as it was. Not Liberation Sans: LibreOffice would substitute it
# for Arial and Calibri and change every RTF/DOCX/XLSX/PPTX golden.
RUN apt-get update && apt-get install -y --no-install-recommends fonts-roboto \
    && rm -rf /var/lib/apt/lists/* && fc-cache -f

CMD ["bash"]
