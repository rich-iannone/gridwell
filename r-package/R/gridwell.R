#' @keywords internal
"_PACKAGE"

#' Parse a gridwell table from a JSON string.
#'
#' @param json A JSON string containing the table IR.
#' @return An external pointer to the parsed table.
#' @export
gw_parse_ir <- function(json) {
    .Call(wrap__gw_parse_ir, json)
}

#' Validate a parsed table.
#'
#' @param table_ptr An external pointer to a parsed table (from `gw_parse_ir()`).
#' @return A character vector of validation errors (empty if valid).
#' @export
gw_validate <- function(table_ptr) {
    .Call(wrap__gw_validate, table_ptr)
}

#' Serialize a parsed table back to JSON.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A JSON string.
#' @export
gw_to_json <- function(table_ptr) {
    .Call(wrap__gw_to_json, table_ptr)
}

#' Render a table to HTML.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return An HTML string.
#' @export
gw_render_html <- function(table_ptr) {
    .Call(wrap__gw_render_html, table_ptr)
}

#' Render a table to LaTeX.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A LaTeX string.
#' @export
gw_render_latex <- function(table_ptr) {
    .Call(wrap__gw_render_latex, table_ptr)
}

#' Render a table to Typst.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A Typst string.
#' @export
gw_render_typst <- function(table_ptr) {
    .Call(wrap__gw_render_typst, table_ptr)
}

#' Render a table to RTF.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return An RTF string.
#' @export
gw_render_rtf <- function(table_ptr) {
    .Call(wrap__gw_render_rtf, table_ptr)
}

#' Render a table to SVG.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return An SVG string.
#' @export
gw_render_svg <- function(table_ptr) {
    .Call(wrap__gw_render_svg, table_ptr)
}

#' Render a table with ANSI escape codes.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A string with ANSI codes.
#' @export
gw_render_ansi <- function(table_ptr) {
    .Call(wrap__gw_render_ansi, table_ptr)
}

#' Render a table to Pandoc AST JSON.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A Pandoc JSON string.
#' @export
gw_render_pandoc <- function(table_ptr) {
    .Call(wrap__gw_render_pandoc, table_ptr)
}

#' Render a table to Quarto-flavored Markdown.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A Quarto Markdown string.
#' @export
gw_render_quarto <- function(table_ptr) {
    .Call(wrap__gw_render_quarto, table_ptr)
}

#' Render a table to a named text format.
#'
#' @param table_ptr An external pointer to a parsed table.
#' @param format One of: "html", "latex", "typst", "rtf", "svg", "ansi", "pandoc", "quarto".
#' @return The rendered string.
#' @export
gw_render <- function(table_ptr, format) {
    .Call(wrap__gw_render, table_ptr, format)
}

#' Render a table to DOCX (returns a raw vector).
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A raw vector containing the DOCX file bytes.
#' @export
gw_render_docx <- function(table_ptr) {
    .Call(wrap__gw_render_docx, table_ptr)
}

#' Render a table to XLSX (returns a raw vector).
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A raw vector containing the XLSX file bytes.
#' @export
gw_render_xlsx <- function(table_ptr) {
    .Call(wrap__gw_render_xlsx, table_ptr)
}

#' Render a table to PPTX (returns a raw vector).
#'
#' @param table_ptr An external pointer to a parsed table.
#' @return A raw vector containing the PPTX file bytes.
#' @export
gw_render_pptx <- function(table_ptr) {
    .Call(wrap__gw_render_pptx, table_ptr)
}

#' Render a table to a named binary format (returns a raw vector).
#'
#' @param table_ptr An external pointer to a parsed table.
#' @param format One of: "docx", "xlsx", "pptx".
#' @return A raw vector containing the file bytes.
#' @export
gw_render_binary <- function(table_ptr, format) {
    .Call(wrap__gw_render_binary, table_ptr, format)
}
