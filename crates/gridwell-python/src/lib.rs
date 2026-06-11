use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::PyBytes;

use gridwell_ir::Table;

/// A parsed gridwell table IR.
///
/// Create via `Table.from_json(json_str)` or `Table.from_dict(dict)`.
#[pyclass(name = "Table")]
struct PyTable {
    inner: Table,
}

#[pymethods]
impl PyTable {
    /// Parse a table from a JSON string.
    #[staticmethod]
    fn from_json(json: &str) -> PyResult<Self> {
        let table =
            Table::from_json(json).map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyTable { inner: table })
    }

    /// Parse a table from a Python dict (serialized to JSON internally).
    #[staticmethod]
    fn from_dict(py: Python<'_>, dict: &Bound<'_, PyAny>) -> PyResult<Self> {
        let json_mod = py.import("json")?;
        let json_str: String = json_mod
            .call_method1("dumps", (dict,))?
            .extract()?;
        Self::from_json(&json_str)
    }

    /// Validate the table IR, returning a list of error messages.
    /// Returns an empty list if the table is valid.
    fn validate(&self) -> Vec<String> {
        self.inner.validate().into_iter().map(|e| e.to_string()).collect()
    }

    /// Serialize the table back to a JSON string.
    fn to_json(&self) -> PyResult<String> {
        self.inner
            .to_json()
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    // ─── Text renderers ───

    /// Render the table to HTML.
    fn render_html(&self) -> PyResult<String> {
        gridwell_writer_html::render_html(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to LaTeX.
    fn render_latex(&self) -> PyResult<String> {
        gridwell_writer_latex::render_latex(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to Typst.
    fn render_typst(&self) -> PyResult<String> {
        gridwell_writer_typst::render_typst(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to RTF.
    fn render_rtf(&self) -> PyResult<String> {
        gridwell_writer_rtf::render_rtf(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to SVG.
    fn render_svg(&self) -> PyResult<String> {
        gridwell_writer_svg::render_svg(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table with ANSI escape codes.
    fn render_ansi(&self) -> PyResult<String> {
        gridwell_writer_ansi::render_ansi(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to Pandoc AST JSON.
    fn render_pandoc(&self) -> PyResult<String> {
        gridwell_writer_pandoc::render_pandoc(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to Quarto-flavored Markdown.
    fn render_quarto(&self) -> PyResult<String> {
        gridwell_writer_quarto::render_quarto(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))
    }

    /// Render the table to a given text format by name.
    ///
    /// Supported: "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto"
    fn render(&self, format: &str) -> PyResult<String> {
        match format {
            "html" => self.render_html(),
            "latex" => self.render_latex(),
            "typst" => self.render_typst(),
            "rtf" => self.render_rtf(),
            "svg" => self.render_svg(),
            "ansi" => self.render_ansi(),
            "pandoc" => self.render_pandoc(),
            "quarto" => self.render_quarto(),
            _ => Err(PyValueError::new_err(format!(
                "Unknown text format: '{format}'. \
                 Supported: html, latex, typst, rtf, svg, ansi, pandoc, quarto"
            ))),
        }
    }

    // ─── Binary renderers ───

    /// Render the table to a DOCX file (returns bytes).
    fn render_docx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = gridwell_writer_docx::render_docx(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Render the table to an XLSX file (returns bytes).
    fn render_xlsx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = gridwell_writer_xlsx::render_xlsx(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Render the table to a PPTX file (returns bytes).
    fn render_pptx<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let bytes = gridwell_writer_pptx::render_pptx(&self.inner)
            .map_err(|e| PyValueError::new_err(format!("{e}")))?;
        Ok(PyBytes::new(py, &bytes))
    }

    /// Render the table to a given binary format by name (returns bytes).
    ///
    /// Supported: "docx", "xlsx", "pptx"
    fn render_binary<'py>(&self, py: Python<'py>, format: &str) -> PyResult<Bound<'py, PyBytes>> {
        match format {
            "docx" => self.render_docx(py),
            "xlsx" => self.render_xlsx(py),
            "pptx" => self.render_pptx(py),
            _ => Err(PyValueError::new_err(format!(
                "Unknown binary format: '{format}'. Supported: docx, xlsx, pptx"
            ))),
        }
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

/// The gridwell Python module: fast multi-format table rendering.
#[pymodule]
#[pyo3(name = "_native")]
fn gridwell(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyTable>()?;
    m.add_function(wrap_pyfunction!(parse_ir, m)?)?;
    Ok(())
}
