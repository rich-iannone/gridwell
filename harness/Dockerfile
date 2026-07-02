# Pinned toolchain for the Gridwell visual test harness.
#
# This image fixes the versions of every external renderer so that the gated
# visual-diff (HTML/SVG/Typst) is reproducible across machines and CI runs. The
# image tag encodes the toolchain set (e.g. gridwell-harness:pinned-2026-07); to
# test a *new* renderer version, build a new tag and add it to the CI matrix in
# .github/workflows/visual.yml — do not mutate an existing tag.
#
# Build:  docker build -f harness/Dockerfile -t gridwell-harness:pinned-2026-07 .
# Run:    docker run --rm -v "$PWD:/w" -w /w gridwell-harness:pinned-2026-07 \
#             cargo xtask gallery --check

# Pinned Rust + Debian bookworm base.
FROM rust:1.83-bookworm

ENV DEBIAN_FRONTEND=noninteractive

# System renderers from the bookworm apt snapshot:
#  - chromium          : HTML screenshots (gated)
#  - librsvg2-bin       : rsvg-convert, SVG -> PNG (gated)
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

# Typst isn't packaged for bookworm; install a pinned CLI via cargo. (SVG uses
# librsvg2-bin's rsvg-convert above, which is more reliable than `cargo install
# resvg`, whose recent releases are library-only.)
RUN cargo install typst-cli --version 0.12.0 --locked

# Chromium in a container must run without the sandbox; the xtask already
# passes --no-sandbox, but export a hint for anything else that shells out.
ENV CHROME_BIN=/usr/bin/chromium

# Warm the font cache so text metrics are stable on first run.
RUN fc-cache -f

CMD ["bash"]
