//! Turning rendered output files into PNG previews by shelling out to external
//! renderers. Every renderer is optional: if the tool is missing the cell is
//! reported as [`RasterOutcome::Unavailable`] rather than failing the run.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::formats::Raster;

/// Result of trying to rasterize one output.
pub enum RasterOutcome {
    /// A PNG was produced at this path.
    Png(PathBuf),
    /// The required tool is not installed.
    Unavailable(&'static str),
    /// The tool ran but failed.
    Failed(String),
}

/// Detected external tools (probed once at startup).
pub struct Tools {
    pub chromium: Option<String>,
    pub resvg: Option<String>,
    pub typst: Option<String>,
    pub xelatex: Option<String>,
    pub soffice: Option<String>,
    pub pdftoppm: Option<String>,
    /// Whether `standalone.cls` is installed (gives cropped LaTeX output).
    pub standalone_cls: bool,
}

impl Tools {
    pub fn detect() -> Self {
        Tools {
            chromium: which(&[
                "chromium",
                "chromium-browser",
                "google-chrome",
                "google-chrome-stable",
                "chrome",
                "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
            ]),
            resvg: which(&["resvg"]),
            typst: which(&["typst"]),
            xelatex: which(&["xelatex", "pdflatex"]),
            soffice: which(&[
                "soffice",
                "libreoffice",
                "/Applications/LibreOffice.app/Contents/MacOS/soffice",
            ]),
            pdftoppm: which(&["pdftoppm"]),
            standalone_cls: has_latex_class("standalone.cls"),
        }
    }

    /// A one-line summary of what is and isn't available.
    pub fn summary(&self) -> String {
        let f = |o: &Option<String>| if o.is_some() { "yes" } else { "no" };
        format!(
            "chromium={} resvg={} typst={} xelatex={} soffice={} pdftoppm={}",
            f(&self.chromium),
            f(&self.resvg),
            f(&self.typst),
            f(&self.xelatex),
            f(&self.soffice),
            f(&self.pdftoppm),
        )
    }
}

/// Rasterize `output_path` (an already-written format file) to `png_path`.
pub fn rasterize(
    raster: Raster,
    tools: &Tools,
    output_path: &Path,
    png_path: &Path,
    work_dir: &Path,
) -> RasterOutcome {
    match raster {
        Raster::Browser => match &tools.chromium {
            Some(bin) => browser(bin, output_path, png_path, work_dir),
            None => RasterOutcome::Unavailable("chromium"),
        },
        Raster::Resvg => match &tools.resvg {
            Some(bin) => run_simple(bin, &[output_path, png_path], png_path),
            None => RasterOutcome::Unavailable("resvg"),
        },
        Raster::Typst => match &tools.typst {
            Some(bin) => typst(bin, output_path, png_path, work_dir),
            None => RasterOutcome::Unavailable("typst"),
        },
        Raster::Latex => match (&tools.xelatex, &tools.pdftoppm) {
            (Some(tex), Some(ppm)) => latex(
                tex,
                ppm,
                tools.standalone_cls,
                output_path,
                png_path,
                work_dir,
            ),
            (None, _) => RasterOutcome::Unavailable("xelatex"),
            (_, None) => RasterOutcome::Unavailable("pdftoppm"),
        },
        Raster::Office => match (&tools.soffice, &tools.pdftoppm) {
            (Some(so), Some(ppm)) => office(so, ppm, output_path, png_path, work_dir),
            (None, _) => RasterOutcome::Unavailable("soffice"),
            (_, None) => RasterOutcome::Unavailable("pdftoppm"),
        },
        Raster::AnsiText | Raster::SourceText => {
            RasterOutcome::Failed("text formats are not rasterized".into())
        }
    }
}

/// A minimal base stylesheet so gridwell's (mostly unstyled) HTML fragment is
/// legible; gridwell's own emitted `<style>` block augments/overrides it. This
/// wrapper is a fixed constant so the golden image stays deterministic.
const HTML_BASE_CSS: &str = "\
body { margin: 0; padding: 16px; background: #fff; font-family: Arial, Helvetica, sans-serif; }
.gw_table { border-collapse: collapse; font-size: 14px; }
.gw_table th, .gw_table td { border: 1px solid #ccc; padding: 4px 8px; text-align: left; }
.gw_table thead th { font-weight: bold; border-bottom: 2px solid #333; }
";

fn browser(bin: &str, html: &Path, png: &Path, work_dir: &Path) -> RasterOutcome {
    let fragment = match std::fs::read_to_string(html) {
        Ok(s) => s,
        Err(e) => return RasterOutcome::Failed(format!("read html: {e}")),
    };
    let wrapped = format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><style>{HTML_BASE_CSS}</style></head><body>{fragment}</body></html>"
    );
    let wrapped_path = work_dir.join("page.html");
    if let Err(e) = std::fs::write(&wrapped_path, wrapped) {
        return RasterOutcome::Failed(format!("write wrapper: {e}"));
    }
    let out = Command::new(bin)
        .args([
            "--headless=new",
            "--no-sandbox",
            "--disable-gpu",
            "--hide-scrollbars",
            "--force-device-scale-factor=2",
            "--default-background-color=ffffffff",
            "--window-size=1200,1600",
        ])
        .arg(format!("--screenshot={}", png.display()))
        .arg(&wrapped_path)
        .output();
    finish(out, png)
}

fn typst(bin: &str, typ: &Path, png: &Path, work_dir: &Path) -> RasterOutcome {
    let src = match std::fs::read_to_string(typ) {
        Ok(s) => s,
        Err(e) => return RasterOutcome::Failed(format!("read typ: {e}")),
    };
    let wrapped = format!("#set page(width: auto, height: auto, margin: 10pt)\n{src}\n");
    let wrapped_path = work_dir.join("doc.typ");
    if let Err(e) = std::fs::write(&wrapped_path, wrapped) {
        return RasterOutcome::Failed(format!("write typ: {e}"));
    }
    let out = Command::new(bin)
        .args(["compile", "--format", "png", "--ppi", "144"])
        .arg(&wrapped_path)
        .arg(png)
        .output();
    finish(out, png)
}

const LATEX_PACKAGES: &str = "\
\\usepackage{booktabs}
\\usepackage{array}
\\usepackage{multirow}
\\usepackage[table]{xcolor}
\\usepackage{colortbl}
";

/// `standalone` crops tightly to the table; `article` is the portable fallback
/// when `standalone.cls` isn't installed (produces a page-sized preview).
fn latex_preamble(standalone: bool) -> String {
    if standalone {
        format!("\\documentclass[border=10pt]{{standalone}}\n{LATEX_PACKAGES}\\begin{{document}}\n")
    } else {
        format!(
            "\\documentclass[12pt]{{article}}\n\
             \\usepackage[margin=12pt,paperwidth=40cm,paperheight=40cm]{{geometry}}\n\
             {LATEX_PACKAGES}\\pagestyle{{empty}}\n\\begin{{document}}\n\\noindent\n"
        )
    }
}

fn latex(
    tex_bin: &str,
    ppm_bin: &str,
    standalone: bool,
    tex: &Path,
    png: &Path,
    work_dir: &Path,
) -> RasterOutcome {
    let body = match std::fs::read_to_string(tex) {
        Ok(s) => s,
        Err(e) => return RasterOutcome::Failed(format!("read tex: {e}")),
    };
    let doc = format!("{}{body}\n\\end{{document}}\n", latex_preamble(standalone));
    let doc_path = work_dir.join("doc.tex");
    if let Err(e) = std::fs::write(&doc_path, doc) {
        return RasterOutcome::Failed(format!("write tex: {e}"));
    }
    let out = Command::new(tex_bin)
        .args(["-interaction=nonstopmode", "-halt-on-error"])
        .arg(format!("-output-directory={}", work_dir.display()))
        .arg(&doc_path)
        .output();
    let pdf = work_dir.join("doc.pdf");
    if let Err(e) = check(out) {
        return RasterOutcome::Failed(format!("xelatex: {e}"));
    }
    if !pdf.exists() {
        return RasterOutcome::Failed("xelatex produced no pdf".into());
    }
    pdf_to_png(ppm_bin, &pdf, png)
}

fn office(so_bin: &str, ppm_bin: &str, input: &Path, png: &Path, work_dir: &Path) -> RasterOutcome {
    // A private profile dir avoids clashing with a running LibreOffice instance.
    let profile = work_dir.join("lo-profile");
    let out = Command::new(so_bin)
        .arg(format!(
            "-env:UserInstallation=file://{}",
            profile.display()
        ))
        .args(["--headless", "--convert-to", "pdf", "--outdir"])
        .arg(work_dir)
        .arg(input)
        .output();
    if let Err(e) = check(out) {
        return RasterOutcome::Failed(format!("soffice: {e}"));
    }
    let stem = input.file_stem().and_then(|s| s.to_str()).unwrap_or("out");
    let pdf = work_dir.join(format!("{stem}.pdf"));
    if !pdf.exists() {
        return RasterOutcome::Failed("soffice produced no pdf".into());
    }
    pdf_to_png(ppm_bin, &pdf, png)
}

fn pdf_to_png(ppm_bin: &str, pdf: &Path, png: &Path) -> RasterOutcome {
    // pdftoppm -singlefile writes <prefix>.png
    let prefix = png.with_extension("");
    let out = Command::new(ppm_bin)
        .args(["-png", "-r", "144", "-singlefile", "-f", "1", "-l", "1"])
        .arg(pdf)
        .arg(&prefix)
        .output();
    finish(out, png)
}

fn run_simple(bin: &str, args: &[&Path], png: &Path) -> RasterOutcome {
    let out = Command::new(bin).args(args).output();
    finish(out, png)
}

/// Turn a command result + expected PNG into an outcome.
fn finish(out: std::io::Result<std::process::Output>, png: &Path) -> RasterOutcome {
    match check(out) {
        Ok(()) if png.exists() => {
            crop_to_content(png);
            RasterOutcome::Png(png.to_path_buf())
        }
        Ok(()) => RasterOutcome::Failed("tool produced no png".into()),
        Err(e) => RasterOutcome::Failed(e),
    }
}

/// Trim the uniform background margin around a rendered PNG so previews show the
/// table, not a sea of whitespace. The background color is taken from the
/// top-left pixel; a small margin is kept. Best-effort and deterministic (so
/// goldens crop identically) — on any error the original is left untouched.
fn crop_to_content(png: &Path) {
    const TOL: u8 = 10;
    const PAD: u32 = 10;

    let img = match image::open(png) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return,
    };
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return;
    }
    let bg = *img.get_pixel(0, 0);
    let differs = |p: &image::Rgba<u8>| {
        p.0.iter()
            .zip(bg.0.iter())
            .any(|(a, b)| a.abs_diff(*b) > TOL)
    };

    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    for (x, y, p) in img.enumerate_pixels() {
        if differs(p) {
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
    }
    if max_x < min_x || max_y < min_y {
        return; // entirely background
    }

    let x0 = min_x.saturating_sub(PAD);
    let y0 = min_y.saturating_sub(PAD);
    let x1 = (max_x + PAD).min(w - 1);
    let y1 = (max_y + PAD).min(h - 1);
    let (cw, ch) = (x1 - x0 + 1, y1 - y0 + 1);
    if cw == w && ch == h {
        return; // nothing to trim
    }
    let cropped = image::imageops::crop_imm(&img, x0, y0, cw, ch).to_image();
    let _ = cropped.save(png);
}

/// Interpret a command result, returning the trimmed stderr on failure.
fn check(out: std::io::Result<std::process::Output>) -> Result<(), String> {
    match out {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => {
            let err = String::from_utf8_lossy(&o.stderr);
            let tail: String = err.lines().rev().take(3).collect::<Vec<_>>().join(" | ");
            Err(if tail.is_empty() {
                format!("exit {}", o.status)
            } else {
                tail
            })
        }
        Err(e) => Err(e.to_string()),
    }
}

/// Find the first available executable among `candidates` (PATH names or
/// absolute paths).
fn which(candidates: &[&str]) -> Option<String> {
    for cand in candidates {
        if cand.contains('/') {
            if Path::new(cand).exists() {
                return Some(cand.to_string());
            }
            continue;
        }
        if let Ok(path) = std::env::var("PATH") {
            for dir in std::env::split_paths(&path) {
                let full = dir.join(cand);
                if full.is_file() {
                    return Some(full.to_string_lossy().into_owned());
                }
            }
        }
    }
    None
}

/// Whether a LaTeX class/style file is installed, via `kpsewhich`.
fn has_latex_class(name: &str) -> bool {
    Command::new("kpsewhich")
        .arg(name)
        .output()
        .map(|o| o.status.success() && !o.stdout.is_empty())
        .unwrap_or(false)
}

/// Strip ANSI escape sequences so terminal output is legible as plain text.
pub fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            // Skip CSI: ESC [ ... letter
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
