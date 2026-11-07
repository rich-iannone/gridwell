"""The gridwell Python module: fast multi-format table rendering."""

# Type stubs for the native module, which is written in Rust. tests/test_typing.py
# checks them against the module: names, signatures, docstrings, and each
# per-format method's options and defaults against `formats()`.

import os
from collections.abc import Sequence
from typing import Any, Literal, TypeAlias, TypedDict, TypeVar, final, type_check_only

from great_tables import GT
from typing_extensions import Self

__all__ = [
    "Table",
    "InvalidTableError",
    "InvalidOptionsError",
    "parse_ir",
    "formats",
    "_conformance_normalize",
    "_conformance_check",
]

_PathT = TypeVar("_PathT", bound=str | os.PathLike[str])

_TableSource: TypeAlias = str | dict[str, Any] | Table | GT

# One entry of `formats()`; for annotations only, it doesn't exist at runtime.
@type_check_only
class FormatInfo(TypedDict):
    name: str
    description: str
    kind: Literal["text", "binary"]
    extension: str
    media_type: str
    experimental: bool
    options: dict[str, Any]

class InvalidTableError(ValueError):
    """Raised when rendering a table whose IR fails validation. Subclass of ValueError;
    the message lists the validation errors."""

class InvalidOptionsError(ValueError):
    """Raised when writer options are not valid for the format (unknown option, wrong
    type, value out of range). Subclass of ValueError."""

@final
class Table:
    """A parsed gridwell table IR.

    Create with `Table(source)` from IR as a JSON string or a dict, or from a Great
    Tables `GT`; or with `Table.from_json`, `Table.from_dict` or `Table.from_gt`.
    In a notebook it displays as HTML, and `print()` draws it as text.
    IR of a newer minor version than this gridwell reads is read with a
    `UserWarning`; another major version is refused.

    A table can't be changed once made, and the work done in Rust (parsing,
    validating, rendering, saving) runs with the GIL released, so other Python
    threads run meanwhile and one table can be rendered on several threads at once.
    """

    def __new__(cls, source: _TableSource) -> Self: ...
    @staticmethod
    def from_gt(gt: GT) -> Table:
        """Convert a Great Tables `GT` (with `gridwell.gt_to_ir`, which needs the `gt`
        extra). Raises `TypeError` for anything else."""

    @staticmethod
    def from_json(json: str) -> Table:
        """Parse a table from a JSON string."""

    @staticmethod
    def from_dict(dict: dict[str, Any]) -> Table:
        """Parse a table from a Python dict (serialized to JSON internally)."""

    def validate(self) -> list[str]:
        """Validate the table IR, returning a list of error messages.
        Returns an empty list if the table is valid."""

    def warnings(self, format: str | None = None) -> list[str]:
        """What the table's output will lack, as messages: first that the table is
        a newer minor IR version than this gridwell reads, if it is; then the
        features a format can't express, and where, for one format by name, or
        every format (`None`). Empty if the table is expressed whole. A warning
        doesn't stop the table rendering.

        Raises `InvalidTableError` for invalid IR, and `ValueError` for an
        unknown format."""

    def to_json(self) -> str:
        """Serialize the table back to a JSON string."""

    def render(self, format: str, options: dict[str, Any] | None = None) -> str:
        """Render the table to a text format by name, with optional writer options
        (a dict; see `gridwell.formats()` for each format's options and defaults).

        Raises `InvalidTableError` for invalid IR, `InvalidOptionsError` for bad
        options, and `ValueError` for an unknown or binary format."""

    def render_binary(self, format: str, options: dict[str, Any] | None = None) -> bytes:
        """Render the table to a binary format ("docx", "xlsx", "pptx"), returning bytes."""

    def save(self, path: _PathT, format: str | None = None, **options: Any) -> _PathT:
        """Render the table and write it to `path` (a str or `os.PathLike`), returning
        `path`. The format is inferred from the extension (`.html`, `.tex`, `.typ`,
        `.rtf`, `.svg`, `.txt` for ansi, `.docx`, `.xlsx`, `.pptx`) unless `format`
        is given; `.json` is ambiguous (pandoc or quarto), so it needs `format`.
        Writer options are keyword arguments. The bytes are written unchanged (text
        as UTF-8, no added newline), and `.txt` files are plain text unless
        `color=True`.

        Raises `InvalidTableError`, `InvalidOptionsError`, `ValueError` for an
        unknown format or an extension it can't infer one from, and `OSError` if the
        file can't be written. Nothing is written if rendering fails."""

    # Per-format shorthands: each format's options as keyword arguments, with the
    # defaults `formats()` gives.

    def render_html(
        self,
        *,
        inline_styles: bool = False,
        pretty_print: bool = True,
        class_prefix: str = "gw",
        base_styles: bool = True,
        table_id: str | None = None,
        standalone: bool = False,
        quarto_disable_processing: bool = False,
    ) -> str:
        """Render the table to HTML. Options: inline_styles, pretty_print, class_prefix,
        base_styles, table_id, standalone, quarto_disable_processing."""

    def render_latex(
        self,
        *,
        longtable: bool = False,
        longtable_threshold: int = 30,
        booktabs: bool = True,
    ) -> str:
        """Render the table to LaTeX. Options: longtable, longtable_threshold, booktabs."""

    def render_typst(self, *, repeat_header: bool = True) -> str:
        """Render the table to Typst. Options: repeat_header."""

    def render_rtf(self) -> str:
        """Render the table to RTF."""

    def render_svg(
        self,
        *,
        font_family: str = "Roboto, Helvetica, Arial, 'Liberation Sans', Arimo, sans-serif",
        font_size: float = 16.0,
        row_height: float = 0.0,
        cell_padding_x: float = 5.0,
        cell_padding_y: float = 8.0,
        default_col_width: float = 120.0,
        base_styles: bool = True,
    ) -> str:
        """Render the table to SVG. Options: font_family, font_size, row_height,
        cell_padding_x, cell_padding_y, default_col_width, base_styles."""

    def render_ansi(
        self,
        *,
        box_drawing: bool = True,
        true_color: bool = True,
        max_width: int = 0,
        background_colors: bool = False,
        color: bool = True,
    ) -> str:
        """Render the table for a terminal. Options: box_drawing, true_color, max_width,
        background_colors, color."""

    def render_pandoc(self) -> str:
        """Render the table to Pandoc AST JSON."""

    def render_quarto(
        self,
        *,
        table_id: str | None = None,
        extra_attrs: Sequence[tuple[str, str]] = ...,
    ) -> str:
        """Render the table to Quarto-flavored Pandoc AST JSON. Options: table_id,
        extra_attrs."""

    def render_docx(self) -> bytes:
        """Render the table to a DOCX file (returns bytes). The format has no options
        yet; any given raise `InvalidOptionsError`."""

    def render_xlsx(self) -> bytes:
        """Render the table to an XLSX file (returns bytes). The format has no options
        yet; any given raise `InvalidOptionsError`."""

    def render_pptx(self) -> bytes:
        """Render the table to a PPTX file (returns bytes). The format has no options
        yet; any given raise `InvalidOptionsError`."""

    def __repr__(self) -> str: ...
    def __str__(self) -> str: ...
    def _repr_html_(self) -> str | None:
        """The table as HTML for a notebook (Jupyter, VS Code, Quarto), with a fresh
        `table_id` so its styles apply to it alone, marked so Quarto doesn't restyle
        it. `None` for an invalid table, so the notebook shows the `repr()` instead."""

def parse_ir(json: str) -> Table:
    """Parse a table IR from a JSON string.

    Shorthand for `Table.from_json(json_str)`."""

def formats() -> list[FormatInfo]:
    """Every supported format: a list of dicts with name, description, kind ("text" or
    "binary"), extension, media_type, experimental (True for a format whose styling
    is best-effort and may change between releases), and options (each option's
    default value)."""

def _conformance_normalize(table: Table, tier: str) -> str:
    """The normalized conformance grid of a table, as JSON (`gridwell.conformance`
    wraps it). Raises `InvalidTableError` for a table that doesn't validate."""

def _conformance_check(ir: str, expected: str, tier: str | None = None) -> str:
    """Check IR (JSON text) against an expected grid (JSON text), giving the report
    as JSON (`gridwell.conformance` wraps it). IR that doesn't parse or validate
    fails L0; an expected grid that isn't one, or a tier it doesn't go to, raises
    `ValueError`."""
