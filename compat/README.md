# IR compatibility corpus

Tables written for each IR version, kept so that every later gridwell that reads that
major version is tested against them. `ir-1.0/` holds tables written for IR 1.0, and
every gridwell that reads IR 1 must read them the same, forever.

| Directory | Where its tables come from |
|---|---|
| `examples/` | the example corpus (`crates/gridwell-testkit/src/examples/`) |
| `fixtures/` | the conformance fixtures, comprehensive and minimal tiers (`fixtures/`) |
| `docs/` | the IR the documentation shows, `maximal.json` (every field of every object) and `freeze-forms.json` (the forms settled at the freeze: fields left out, a node's `fallback`, `string` and `duration` typed values) |
| `gt/` | IR written by gt's emitter (the conformance kit's references) |
| `great-tables/` | IR written by Great Tables' emitter (the same) |

Each table `name.json` has its grid, `name.grid.json`: the conformance kit's
normalized L3 grid (structure, content and resolved styling; see
[`conformance/README.md`](../conformance/README.md)) as gridwell read the table when
it was frozen.

## What the test checks

`crates/gridwell-render/tests/compat.rs` runs with `cargo test`. For every table:

- It parses, has the version its directory says, and validates.
- It still reads as its grid says.
- It renders in every format.
- It writes back to IR that reads the same, and writing it back again gives the same
  JSON.
- With what a newer minor version may add, it reads the same: an unknown field of
  every JSON kind on every object; a newer minor `ir_version` (with a
  `NEWER_IR_VERSION` warning). On `docs/maximal.json` it also checks that every
  format's output is byte-for-byte the same with unknown fields everywhere, and with
  an unknown node at the start and end of every content list, which is drawn as
  nothing and written back whole.

`MANIFEST` holds the hash of every file, so a frozen table or grid can't be edited or
removed to make a change pass.

## Adding tables

Tables are only ever added. Put the new table in, then write its grid and its
`MANIFEST` line:

```bash
GRIDWELL_BLESS_COMPAT=1 cargo test -p gridwell-render --test compat
```

That writes the grid of a table that has none, and adds files that aren't in
`MANIFEST` yet. It never changes an existing grid or `MANIFEST` line.

A new minor version gets its own directory, `ir-1.1/`, of tables that use what it
adds, and its version goes into `VERSIONS` in the test. The 1.0 tables keep being
tested as they are.

## A new minor version

What a minor version may change is in
[the IR docs](../docs/schema/index.qmd#what-a-minor-version-may-change): it only
adds (optional fields, content node types, keyword values, length units, color
forms, link schemes and selector fields), and every new field has a default that
does what the earlier version did without it. To make IR 1.1:

1. Set `IR_VERSION` to `"1.1"` (`crates/gridwell-ir/src/version.rs`), so gridwell
   writes it. Tables of 1.0 keep reading as they did; this directory's tests say so.
2. Add the fields or node types, with their validation, writers and tests.
   Give each new field a value in `xtask/tests/docs/maximal.json`, which has every
   field of every object, and a row in its object's field table in `docs/schema/`.
3. Copy `crates/gridwell-ir/schema/ir-1.0.schema.json` to `ir-1.1.schema.json`. Set
   its `$id` and `title`, add what 1.1 adds (each field with its `default`; a node
   type as a new definition, a last `anyOf` branch of `content_node` and an entry in
   `unknown_node`'s list), and point `JSON_SCHEMA` (`crates/gridwell-ir/src/lib.rs`)
   at it. Copy it to `docs/schema/`, which serves it at its `$id`. Never edit
   `ir-1.0.schema.json`: `xtask/tests/schema.rs` checks that each schema takes all
   that the ones before it take.
4. Make `ir-1.1/` here, of tables that use what 1.1 adds, add `"1.1"` to `VERSIONS`
   in `crates/gridwell-render/tests/compat.rs`, and bless it as above.
5. Add 1.1 to the version table in `docs/schema/index.qmd`, with what it adds.
6. The gt and Great Tables emitters write `"1.1"` once they use what it adds, and the
   conformance kit's references are written again by emitters that do.

A gridwell that reads only 1.0 still renders a 1.1 table, without what 1.1 added,
and warns of it (`NEWER_IR_VERSION`).

## When a test fails

A failure means gridwell reads IR written for an older version differently, which is
the break the version promise rules out. Fix gridwell, not the table.

The one exception is a fix to how gridwell reads a table, where the frozen grid
recorded a bug. Then delete the grid and its `MANIFEST` line, write it again with
`GRIDWELL_BLESS_COMPAT=1`, and say in the commit what was wrong and why the change
can't break a table someone has written. A change to the grid format itself (a new
`gridwell-conformance-grid` version) rewrites every grid the same way.

Every file here has LF line ends (see `.gitattributes`), so the hashes hold on any
checkout.
