"""Performance comparison: Great Tables native rendering vs gridwell."""

import time

import pandas as pd
from great_tables import GT

import gridwell


def make_large_table(n_rows: int = 1000, n_cols: int = 10) -> pd.DataFrame:
    """Create a large DataFrame for benchmarking."""
    data = {}
    for i in range(n_cols):
        if i % 3 == 0:
            data[f"num_{i}"] = [float(j) * 1.1 for j in range(n_rows)]
        elif i % 3 == 1:
            data[f"str_{i}"] = [f"value_{j}" for j in range(n_rows)]
        else:
            data[f"int_{i}"] = list(range(n_rows))
    return pd.DataFrame(data)


def bench(name: str, fn, iterations: int = 5):
    """Run a benchmark and print results."""
    times = []
    for _ in range(iterations):
        start = time.perf_counter()
        fn()
        elapsed = time.perf_counter() - start
        times.append(elapsed)

    avg = sum(times) / len(times)
    best = min(times)
    print(f"  {name:40s}  avg={avg*1000:8.2f}ms  best={best*1000:8.2f}ms")
    return avg


def main():
    print("=" * 70)
    print("Performance Comparison: Great Tables vs Gridwell")
    print("=" * 70)

    for n_rows in [100, 500, 1000]:
        df = make_large_table(n_rows=n_rows)
        gt_obj = GT(df)

        print(f"\n--- {n_rows} rows x {len(df.columns)} columns ---\n")

        # Great Tables native HTML render
        def gt_html():
            gt_obj.as_raw_html()

        # Gridwell pipeline: emit IR + parse + render HTML
        ir_json = gridwell.gt_to_ir(gt_obj)
        table = gridwell.Table.from_json(ir_json)

        def gw_html():
            table.render_html()

        def gw_full():
            t = gridwell.Table.from_json(gridwell.gt_to_ir(gt_obj))
            t.render_html()

        def gw_ir_emit():
            gridwell.gt_to_ir(gt_obj)

        def gw_parse():
            gridwell.Table.from_json(ir_json)

        gt_time = bench("Great Tables as_raw_html()", gt_html)
        bench("Gridwell render_html() (pre-parsed)", gw_html)
        bench("Gridwell full pipeline (emit+parse+render)", gw_full)
        bench("  - IR emission only", gw_ir_emit)
        bench("  - parse_ir only", gw_parse)

        # Speedup
        gw_time = bench("Gridwell render_html() [repeat]", gw_html)
        if gw_time > 0:
            print(f"\n  Speedup (render only): {gt_time / gw_time:.1f}x")

    print("\n" + "=" * 70)
    print("Note: Gridwell render is pure Rust; IR emission is Python overhead.")
    print("=" * 70)


if __name__ == "__main__":
    main()
