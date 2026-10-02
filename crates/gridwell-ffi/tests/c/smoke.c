/*
 * C-side smoke test for the gridwell C API, compiled and run by tests/c_api.rs.
 *
 * Usage: smoke <fixtures-dir>
 * Exits 0 on success; on failure prints the failing check and exits 1.
 * Also compiled as C++ to check the header's cpp_compat.
 */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "gridwell.h"

static int failures = 0;

#define CHECK(cond)                                                          \
    do {                                                                     \
        if (!(cond)) {                                                       \
            fprintf(stderr, "%s:%d: CHECK failed: %s\n", __FILE__, __LINE__, \
                    #cond);                                                  \
            failures++;                                                      \
        }                                                                    \
    } while (0)

static char *read_file(const char *dir, const char *rel, size_t *len) {
    char path[4096];
    FILE *f;
    long size;
    char *buf;

    snprintf(path, sizeof path, "%s/%s", dir, rel);
    f = fopen(path, "rb");
    if (!f) {
        fprintf(stderr, "cannot open %s\n", path);
        exit(2);
    }
    fseek(f, 0, SEEK_END);
    size = ftell(f);
    fseek(f, 0, SEEK_SET);
    buf = (char *)malloc((size_t)size + 1);
    if (fread(buf, 1, (size_t)size, f) != (size_t)size) {
        fprintf(stderr, "cannot read %s\n", path);
        exit(2);
    }
    buf[size] = '\0';
    fclose(f);
    *len = (size_t)size;
    return buf;
}

static GridwellTable *parse_fixture(const char *dir, const char *rel) {
    size_t len;
    char *json = read_file(dir, rel, &len);
    GridwellError *err = NULL;
    GridwellTable *table = gridwell_parse_ir(json, len, &err);
    free(json); /* the table owns its own copy */
    CHECK(table != NULL);
    CHECK(err == NULL);
    return table;
}

static void test_text_formats(const char *dir) {
    static const char *formats[] = {"html", "latex", "typst", "rtf",
                                    "svg",  "ansi",  "pandoc", "quarto"};
    GridwellTable *table = parse_fixture(dir, "comprehensive/reference_table.json");
    size_t i;

    CHECK(gridwell_validate(table) == NULL);
    for (i = 0; i < sizeof formats / sizeof formats[0]; i++) {
        GridwellError *err = NULL;
        GridwellTextResult r = gridwell_render_text(table, formats[i], &err);
        CHECK(err == NULL);
        CHECK(r.text != NULL);
        CHECK(r.len > 0);
        CHECK(strlen(r.text) == r.len); /* NUL-terminated, no interior NULs */
        gridwell_free_text_result(r);
    }
    gridwell_free_table(table);
}

static void test_binary_formats(const char *dir) {
    static const char *formats[] = {"docx", "xlsx", "pptx"};
    GridwellTable *table = parse_fixture(dir, "comprehensive/reference_table.json");
    size_t i;

    for (i = 0; i < sizeof formats / sizeof formats[0]; i++) {
        GridwellError *err = NULL;
        GridwellBinaryResult r = gridwell_render_binary(table, formats[i], &err);
        CHECK(err == NULL);
        CHECK(r.data != NULL);
        CHECK(r.len > 4);
        CHECK(r.data[0] == 'P' && r.data[1] == 'K'); /* zip */
        gridwell_free_binary_result(r);
    }
    gridwell_free_table(table);
}

static void test_invalid_ir_is_refused(const char *dir) {
    GridwellTable *table = parse_fixture(dir, "invalid/span_overflow_right.json");
    GridwellError *err = gridwell_validate(table);
    GridwellTextResult t;
    GridwellBinaryResult b;

    CHECK(err != NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_VALIDATE);
    CHECK(strstr(gridwell_error_message(err), "[SPAN_OVERFLOW_RIGHT]") != NULL);
    gridwell_free_error(err);

    err = NULL;
    t = gridwell_render_text(table, "html", &err);
    CHECK(t.text == NULL && t.len == 0);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_VALIDATE);
    gridwell_free_error(err);

    err = NULL;
    b = gridwell_render_binary(table, "docx", &err);
    CHECK(b.data == NULL && b.len == 0);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_VALIDATE);
    gridwell_free_error(err);

    /* Freeing failed (null) results is a no-op. */
    gridwell_free_text_result(t);
    gridwell_free_binary_result(b);
    gridwell_free_table(table);
}

static void test_errors(const char *dir) {
    const char *bad = "{ not json";
    GridwellError *err = NULL;
    GridwellTable *table = gridwell_parse_ir(bad, strlen(bad), &err);
    GridwellTextResult t;

    CHECK(table == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_PARSE);
    CHECK(strlen(gridwell_error_message(err)) > 0);
    gridwell_free_error(err);

    /* Invalid UTF-8 */
    err = NULL;
    table = gridwell_parse_ir("\xff\xfe", 2, &err);
    CHECK(table == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_PARSE);
    gridwell_free_error(err);

    /* Null arguments */
    err = NULL;
    CHECK(gridwell_parse_ir(NULL, 0, &err) == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_INVALID_ARG);
    gridwell_free_error(err);

    err = gridwell_validate(NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_INVALID_ARG);
    gridwell_free_error(err);

    table = parse_fixture(dir, "minimal/minimal_1x1.json");

    err = NULL;
    t = gridwell_render_text(table, NULL, &err);
    CHECK(t.text == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_INVALID_ARG);
    gridwell_free_error(err);

    err = NULL;
    t = gridwell_render_text(NULL, "html", &err);
    CHECK(t.text == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_INVALID_ARG);
    gridwell_free_error(err);

    /* Unknown format */
    err = NULL;
    t = gridwell_render_text(table, "docx", &err);
    CHECK(t.text == NULL);
    CHECK(gridwell_error_code(err) == GRIDWELL_ERR_RENDER);
    gridwell_free_error(err);

    /* A null err out-pointer is allowed. */
    t = gridwell_render_text(table, "nope", NULL);
    CHECK(t.text == NULL);

    /* Null handles */
    CHECK(gridwell_error_code(NULL) == 0);
    CHECK(strcmp(gridwell_error_message(NULL), "(null error)") == 0);
    gridwell_free_error(NULL);
    gridwell_free_table(NULL);
    gridwell_free_table(table);
}

int main(int argc, char **argv) {
    if (argc != 2) {
        fprintf(stderr, "usage: %s <fixtures-dir>\n", argv[0]);
        return 2;
    }
    test_text_formats(argv[1]);
    test_binary_formats(argv[1]);
    test_invalid_ir_is_refused(argv[1]);
    test_errors(argv[1]);

    if (failures) {
        fprintf(stderr, "%d check(s) failed\n", failures);
        return 1;
    }
    printf("ok\n");
    return 0;
}
