use pyo3::create_exception;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use gridwell_ir::Table;
use gridwell_render::{RenderError, REGISTRY};

create_exception!(
    gridwell,
    InvalidTableError,
    PyValueError,
    "Raised when rendering a table whose IR fails validation. Subclass of ValueError; \
     the message lists the validation errors."
);

create_exception!(
    gridwell,
    InvalidOptionsError,
    PyValueError,
    "Raised when writer options are not valid for the format (unknown option, wrong \
     type, value out of range). Subclass of ValueError."
);

fn to_py_err(e: RenderError) -> PyErr {
    match e {
        RenderError::InvalidTable(_) => InvalidTableError::new_err(e.to_string()),
        RenderError::InvalidOptions { .. } => InvalidOptionsError::new_err(e.to_string()),
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

/// A parsed gridwell table IR.
///
/// Create via `Table.from_json(json_str)` or `Table.from_dict(dict)`.
#[pyclass(name = "Table")]
struct PyTable {
    inner: Table,
}

impl PyTable {
    fn text(
        &self,
        py: Python<'_>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<String> {
        let options = options_json(py, options)?;
        gridwell_render::render_text(&self.inner, format, options.as_deref()).map_err(to_py_err)
    }

    fn binary<'py>(
        &self,
        py: Python<'py>,
        format: &str,
        options: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Bound<'py, PyBytes>> {
        let options = options_json(py, options)?;
        let bytes = gridwell_render::render_binary(&self.inner, format, options.as_deref())
            .map_err(to_py_err)?;
        Ok(PyBytes::new(py, &bytes))
    }
}

#[pymethods]
impl PyTable {
    /// Parse a table from a JSON string.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let table = Table::from_json(json).map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyTable { inner: table })
    }

    /// Parse a table from a Python dict (serialized to JSON internally).
    #[staticmethod]
    fn from_dict(py: Python<'_>, dict: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json_mod = py.import("json")?;
        let json_str: String = json_mod.call_method1("dumps", (dict,))?.extract()?;
        Self::from_json(&json_str)
    }

    /// Validate the table IR, returning a list of error messages.
    /// Returns an empty list if the table is valid.
    fn validate(&self) -> Vec<String> {
        self.inner
            .validate()
            .into_iter()
            .map(|e| e.to_string())
            .collect()
    }

    /// Serialize the table back to a JSON string.
    fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
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

    // ─── Per-format shorthands: options as keyword arguments ───

    /// Render the table to HTML. Options: inline_styles, pretty_print, class_prefix.
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
    /// cell_padding_x, cell_padding_y, default_col_width.
    #[pyo3(signature = (**options))]
    fn render_svg(&self, py: Python<'_>, options: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        self.text(py, "svg", options.map(|d| d.as_any()))
    }

    /// Render the table for a terminal. Options: box_drawing, true_color, max_width,
    /// background_colors.
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

    /// Render the table to Quarto-flavoured Pandoc AST JSON. Options: table_id,
    /// extra_attrs.
    #[pyo3(signature = (**options))]
    fn render_quarto(
        &self,
        py: Python<'_>,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<String> {
        self.text(py, "quarto", options.map(|d| d.as_any()))
    }

    /// Render the table to a DOCX file (returns bytes).
    fn render_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "docx", None)
    }

    /// Render the table to an XLSX file (returns bytes).
    fn render_xlsx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "xlsx", None)
    }

    /// Render the table to a PPTX file (returns bytes).
    fn render_pptx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        self.binary(py, "pptx", None)
    }

    fn __repr__(&self) -> String {
        format!("Table(ir_version='{}')", self.inner.ir_version)
    }
}

/// Parse a table IR from a JSON string.
///
/// Shorthand for `Table.from_json(json_str)`.
#[pyfunction]
fn parse_ir(json: &str) -> PyResult<PyTable> {
    PyTable::from_json(json)
}

/// Every supported format: a list of dicts with name, description, kind ("text" or
/// "binary"), extension, media_type, and options (each option's default value).
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
            let options = json.call_method1("loads", (w.default_options().to_string(),))?;
            d.set_item("options", options)?;
            Ok(d)
        })
        .collect()
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
    Ok(())
}
