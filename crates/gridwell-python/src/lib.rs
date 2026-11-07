use pyo3::create_exception;
use std::ffi::CString;
use std::path::PathBuf;

use pyo3::exceptions::{PyOSError, PyTypeError, PyUserWarning, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyString};

use gridwell_ir::Table;
use gridwell_render::conformance::{self, Tier};
use gridwell_render::{RenderError, REGISTRY};

create_exception!(
    gridwell,
    InvalidTableError,
    PyValueError,
    "Raised when rendering a table whose IR fails validation. Subclass of ValueError;\n\
     the message lists the validation errors."
);

create_exception!(
    gridwell,
    InvalidOptionsError,
    PyValueError,
    "Raised when writer options are not valid for the format (unknown option, wrong\n\
     type, value out of range). Subclass of ValueError."
);

fn to_py_err(e: RenderError) -> PyErr {
    match e {
        RenderError::InvalidTable(_) => InvalidTableError::new_err(e.to_string()),
        RenderError::InvalidOptions { .. } => InvalidOptionsError::new_err(e.to_string()),
        RenderError::Io { .. } => PyOSError::new_err(e.to_string()),
        RenderError::NoExtension { .. }
        | RenderError::UnknownExtension { .. }
        | RenderError::AmbiguousExtension { .. } => {
            PyValueError::new_err(format!("{e} with format="))
        }
        _ => PyValueError::new_err(e.to_string()),
    }
}

/// Writer options as JSON: a dict (or `None`) from Python, serialized with the
/// standard `json` module.
fn options_json(py: Python<'_>, options: Option<&Bound<'_, PyAny>>) -> PyResult<Option<String>> {
    match options {
        None => Ok(None),
        Some(o) if o.is_none() => Ok(None),
        Some(o) => {
            if !o.is_instance_of::<PyDict>() {
                return Err(InvalidOptionsError::new_err(format!(
                    "options must be a dict, got {}",
                    o.get_type().name()?
                )));
            }
            let json = py.import("json")?;
            Ok(Some(json.call_method1("dumps", (o,))?.extract()?))
        }
    }
}

/// Whether `obj` is a Great Tables `GT`. Without importing great_tables: if it
/// isn't imported, nothing is a `GT`.
fn is_gt(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<bool> {
    let modules = py.import("sys")?.getattr("modules")?;
    match modules.call_method1("get", ("great_tables",))? {
        m if m.is_none() => Ok(false),
        m => match m.getattr("GT") {
            Ok(gt) => obj.is_instance(&gt),
            Err(_) => Ok(false),
        },
    }
}

/// A parsed gridwell table IR.
///
/// Create with `Table(source)` from IR as a JSON string or a dict, or from a Great
/// Tables `GT`; or with `Table.from_json`, `Table.from_dict` or `Table.from_gt`.
/// In a notebook it displays as HTML, and `print()` draws it as text.
/// IR of a newer minor version than this gridwell reads is read with a
/// `UserWarning`; another major version is refused.
///
/// A table can't be changed once made, and the work done in Rust (parsing,
/// validating, rendering, saving) runs with the GIL released, so other Python
/// threads run meanwhile and one table can be rendered on several threads at once.
#[pyclass(name = "Table", module = "gridwell", frozen)]
struct PyTable {
    inner: Table,
}

impl PyTable {
    /// Parse IR JSON, with the GIL released. A table of a newer minor IR
    /// version warns (a `UserWarning`, as `warnings()` words it).
    fn parse(py: Python<'_>, json: &str) -> PyResult<Self> {
        let table = py
            .allow_threads(|| Table::from_json(json))
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        if table.is_newer_minor() {
            let warning = gridwell_render::Warning::NewerIrVersion {
                version: table.ir_version.clone(),
            };
            let message = CString::new(warning.to_string())
                .map_err(|e| PyValueError::new_err(e.to_string()))?;
            PyErr::warn(py, &py.get_type::<PyUserWarning>(), &message, 1)?;
        }
        Ok(PyTable { inner: table })
    }

    fn text(
        &self,
        py: Python<'_>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        let options = options_json(py, options)?;
        py.allow_threads(|| gridwell_render::render_text(&self.inner, format, options.as_deref()))
            .map_err(to_py_err)
    }

    fn binary<'py>(
        &self,
        py: Python<'py>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let options = options_json(py, options)?;
        let bytes = py
            .allow_threads(|| {
                gridwell_render::render_binary(&self.inner, format, options.as_deref())
            })
            .map_err(to_py_err)?;
        Ok(PyBytes::new(py, &bytes))
    }

    fn errors(&self) -> Vec<String> {
        self.inner
            .validate()
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    fn description(&self) -> String {
        format!(
            "<gridwell.Table: {}>",
            gridwell_render::describe(&self.inner)
        )
    }
}

#[pymethods]
impl PyTable {
    /// A table from IR as a JSON string or a dict, or from a Great Tables `GT`
    /// (converted with `gridwell.gt_to_ir`, which needs the `gt` extra). A
    /// `Table` gives a copy.
    ///
    /// Raises `ValueError` for IR that doesn't parse, and `TypeError` for anything
    /// else.
    #[new]
    fn new(py: Python<'_>, source: &Bound<'_, PyAny>) -> PyResult<Self> {
        if source.is_instance_of::<PyString>() {
            Self::parse(py, source.extract::<&str>()?)
        } else if source.is_instance_of::<PyDict>() {
            Self::from_dict(py, source)
        } else if let Ok(table) = source.downcast::<PyTable>() {
            let original = &table.get().inner;
            Ok(PyTable {
                inner: py.allow_threads(|| original.clone()),
            })
        } else if is_gt(py, source)? {
            Self::from_gt(py, source)
        } else {
            Err(PyTypeError::new_err(format!(
                "Table() takes IR as a JSON string or a dict, or a great_tables.GT; got {}",
                source.get_type().name()?
            )))
        }
    }

    /// Convert a Great Tables `GT` (with `gridwell.gt_to_ir`, which needs the `gt`
    /// extra). Raises `TypeError` for anything else.
    #[staticmethod]
    fn from_gt(py: Python<'_>, gt: &Bound<'_, PyAny>) -> PyResult<Self> {
        if !is_gt(py, gt)? {
            return Err(PyTypeError::new_err(format!(
                "Table.from_gt() takes a great_tables.GT; got {}",
                gt.get_type().name()?
            )));
        }
        let json = py.import("gridwell")?.getattr("gt_to_ir")?.call1((gt,))?;
        Self::parse(py, json.extract::<&str>()?)
    }

    /// Parse a table from a JSON string.
    #[staticmethod]
    fn from_json(py: Python<'_>, json: &str) -> PyResult<Self> {
        Self::parse(py, json)
    }

    /// Parse a table from a Python dict (serialized to JSON internally).
    #[staticmethod]
    fn from_dict(py: Python<'_>, dict: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json_mod = py.import("json")?;
        let json_str = json_mod.call_method1("dumps", (dict,))?;
        Self::parse(py, json_str.extract::<&str>()?)
    }

    /// Validate the table IR, returning a list of error messages.
    /// Returns an empty list if the table is valid.
    fn validate(&self, py: Python<'_>) -> Vec<String> {
        py.allow_threads(|| self.errors())
    }

    /// What the table's output will lack, as messages: first that the table is
    /// a newer minor IR version than this gridwell reads, if it is; then the
    /// features a format can't express, and where, for one format by name, or
    /// every format (`None`). Empty if the table is expressed whole. A warning
    /// doesn't stop the table rendering.
    ///
    /// Raises `InvalidTableError` for invalid IR, and `ValueError` for an
    /// unknown format.
    #[pyo3(signature = (format=None))]
    fn warnings(&self, py: Python<'_>, format: Option<&str>) -> PyResult<Vec<String>> {
        py.allow_threads(|| {
            let found = gridwell_render::warnings(&self.inner, format)?;
            Ok(found.iter().map(|w| w.to_string()).collect())
        })
        .map_err(to_py_err)
    }

    /// Serialize the table back to a JSON string.
    fn to_json(&self, py: Python<'_>) -> PyResult<String> {
        py.allow_threads(|| self.inner.to_json())
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to a text format by name, with optional writer options
    /// (a dict; see `gridwell.formats()` for each format's options and defaults).
    ///
    /// Raises `InvalidTableError` for invalid IR, `InvalidOptionsError` for bad
    /// options, and `ValueError` for an unknown or binary format.
    #[pyo3(signature = (format, options=None))]
    fn render(
        &self,
        py: Python<'_>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        self.text(py, format, options)
    }

    /// Render the table to a binary format ("docx", "xlsx", "pptx"), returning bytes.
    #[pyo3(signature = (format, options=None))]
    fn render_binary<'py>(
        &self,
        py: Python<'py>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, format, options)
    }

    /// Render the table and write it to `path` (a str or `os.PathLike`), returning
    /// `path`. The format is inferred from the extension (`.html`, `.tex`, `.typ`,
    /// `.rtf`, `.svg`, `.txt` for ansi, `.docx`, `.xlsx`, `.pptx`) unless `format`
    /// is given; `.json` is ambiguous (pandoc or quarto), so it needs `format`.
    /// Writer options are keyword arguments. The bytes are written unchanged (text
    /// as UTF-8, no added newline), and `.txt` files are plain text unless
    /// `color=True`.
    ///
    /// Raises `InvalidTableError`, `InvalidOptionsError`, `ValueError` for an
    /// unknown format or an extension it can't infer one from, and `OSError` if the
    /// file can't be written. Nothing is written if rendering fails.
    #[pyo3(signature = (path, format=None, **options))]
    fn save<'py>(
        &self,
        py: Python<'py>,
        path: Bound<'py, PyAny>,
        format: Option<&str>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let target: PathBuf = path.extract()?;
        let options = options_json(py, options.map(|d| d.as_any()))?;
        py.allow_threads(|| {
            gridwell_render::save(&self.inner, &target, format, options.as_deref())
        })
        .map_err(to_py_err)?;
        Ok(path)
    }

    // ─── Per-format shorthands: options as keyword arguments ───

    /// Render the table to HTML. Options: inline_styles, pretty_print, class_prefix,
    /// base_styles, table_id, standalone, quarto_disable_processing.
    #[pyo3(signature = (**options))]
    fn render_html(&self, py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        self.text(py, "html", options.map(|d| d.as_any()))
    }

    /// Render the table to LaTeX. Options: longtable, longtable_threshold, booktabs.
    #[pyo3(signature = (**options))]
    fn render_latex(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        self.text(py, "latex", options.map(|d| d.as_any()))
    }

    /// Render the table to Typst. Options: repeat_header.
    #[pyo3(signature = (**options))]
    fn render_typst(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        self.text(py, "typst", options.map(|d| d.as_any()))
    }

    /// Render the table to RTF.
    #[pyo3(signature = (**options))]
    fn render_rtf(&self, py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        self.text(py, "rtf", options.map(|d| d.as_any()))
    }

    /// Render the table to SVG. Options: font_family, font_size, row_height,
    /// cell_padding_x, cell_padding_y, default_col_width, base_styles.
    #[pyo3(signature = (**options))]
    fn render_svg(&self, py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        self.text(py, "svg", options.map(|d| d.as_any()))
    }

    /// Render the table for a terminal. Options: box_drawing, true_color, max_width,
    /// background_colors, color.
    #[pyo3(signature = (**options))]
    fn render_ansi(&self, py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        self.text(py, "ansi", options.map(|d| d.as_any()))
    }

    /// Render the table to Pandoc AST JSON.
    #[pyo3(signature = (**options))]
    fn render_pandoc(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        self.text(py, "pandoc", options.map(|d| d.as_any()))
    }

    /// Render the table to Quarto-flavored Pandoc AST JSON. Options: table_id,
    /// extra_attrs.
    #[pyo3(signature = (**options))]
    fn render_quarto(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        self.text(py, "quarto", options.map(|d| d.as_any()))
    }

    /// Render the table to a DOCX file (returns bytes). The format has no options
    /// yet; any given raise `InvalidOptionsError`.
    #[pyo3(signature = (**options))]
    fn render_docx<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "docx", options.map(|d| d.as_any()))
    }

    /// Render the table to an XLSX file (returns bytes). The format has no options
    /// yet; any given raise `InvalidOptionsError`.
    #[pyo3(signature = (**options))]
    fn render_xlsx<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "xlsx", options.map(|d| d.as_any()))
    }

    /// Render the table to a PPTX file (returns bytes). The format has no options
    /// yet; any given raise `InvalidOptionsError`.
    #[pyo3(signature = (**options))]
    fn render_pptx<'py>(
        &self,
        py: Python<'py>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "pptx", options.map(|d| d.as_any()))
    }

    // ─── Display ───

    /// What the table is: its size, row groups, summary rows and title, and
    /// whether it's invalid.
    fn __repr__(&self, py: Python<'_>) -> String {
        py.allow_threads(|| self.description())
    }

    /// The table drawn as plain text (ANSI without color), for `print()`; an invalid
    /// table gives its description and errors instead. Never raises.
    fn __str__(&self, py: Python<'_>) -> String {
        py.allow_threads(|| {
            let plain = serde_json::json!({ "color": false });
            match gridwell_render::render(&self.inner, "ansi", Some(&plain)) {
                Ok(out) => {
                    // `print()` adds the line's end.
                    let text = out.as_text().unwrap_or_default();
                    text.strip_suffix('\n').unwrap_or(text).to_string()
                }
                Err(_) => {
                    let mut lines = vec![self.description()];
                    lines.extend(self.errors().into_iter().map(|e| format!("  {e}")));
                    lines.join("\n")
                }
            }
        })
    }

    /// The table as HTML for a notebook (Jupyter, VS Code, Quarto), with a fresh
    /// `table_id` so its styles apply to it alone, marked so Quarto doesn't restyle
    /// it. `None` for an invalid table, so the notebook shows the `repr()` instead.
    fn _repr_html_(&self, py: Python<'_>) -> Option<String> {
        py.allow_threads(|| gridwell_render::display_html(&self.inner).ok())
    }
}

/// Parse a table IR from a JSON string.
///
/// Shorthand for `Table.from_json(json_str)`.
#[pyfunction]
fn parse_ir(py: Python<'_>, json: &str) -> PyResult<PyTable> {
    PyTable::parse(py, json)
}

/// Every supported format: a list of dicts with name, description, kind ("text" or
/// "binary"), extension, media_type, experimental (True for a format whose styling
/// is best-effort and may change between releases), and options (each option's
/// default value).
#[pyfunction]
fn formats(py: Python<'_>) -> PyResult<Vec<Bound<'_, PyDict>>> {
    let json = py.import("json")?;
    REGISTRY
        .iter()
        .map(|w| {
            let d = PyDict::new(py);
            d.set_item("name", w.name())?;
            d.set_item("description", w.description())?;
            d.set_item("kind", w.kind().to_string())?;
            d.set_item("extension", w.extension())?;
            d.set_item("media_type", w.media_type())?;
            d.set_item("experimental", w.experimental())?;
            let options = json.call_method1("loads", (w.default_options().to_string(),))?;
            d.set_item("options", options)?;
            Ok(d)
        })
        .collect()
}

fn parse_tier(tier: &str) -> PyResult<Tier> {
    tier.parse()
        .map_err(|e: conformance::ConformanceError| PyValueError::new_err(e.to_string()))
}

/// The normalized conformance grid of a table, as JSON (`gridwell.conformance`
/// wraps it). Raises `InvalidTableError` for a table that doesn't validate.
#[pyfunction]
fn _conformance_normalize(py: Python<'_>, table: &PyTable, tier: &str) -> PyResult<String> {
    let tier = parse_tier(tier)?;
    py.allow_threads(|| -> Result<String, String> {
        let validated = table
            .inner
            .validated()
            .map_err(|e| RenderError::InvalidTable(e).to_string())?;
        Ok(conformance::normalize(&validated, tier).to_string())
    })
    .map_err(InvalidTableError::new_err)
}

/// Check IR (JSON text) against an expected grid (JSON text), giving the report
/// as JSON (`gridwell.conformance` wraps it). IR that doesn't parse or validate
/// fails L0; an expected grid that isn't one, or a tier it doesn't go to, raises
/// `ValueError`.
#[pyfunction]
#[pyo3(signature = (ir, expected, tier=None))]
fn _conformance_check(
    py: Python<'_>,
    ir: &str,
    expected: &str,
    tier: Option<&str>,
) -> PyResult<String> {
    let tier = tier.map(parse_tier).transpose()?;
    py.allow_threads(|| conformance::check_json(ir, expected, tier))
        .map(|r| r.to_json().to_string())
        .map_err(|e| PyValueError::new_err(e.to_string()))
}

/// The gridwell Python module: fast multi-format table rendering.
#[pymodule]
#[pyo3(name = "_native")]
fn gridwell(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTable>()?;
    m.add("InvalidTableError", m.py().get_type::<InvalidTableError>())?;
    m.add(
        "InvalidOptionsError",
        m.py().get_type::<InvalidOptionsError>(),
    )?;
    m.add_function(wrap_pyfunction!(parse_ir, m)?)?;
    m.add_function(wrap_pyfunction!(formats, m)?)?;
    m.add_function(wrap_pyfunction!(_conformance_normalize, m)?)?;
    m.add_function(wrap_pyfunction!(_conformance_check, m)?)?;
    Ok(())
}
