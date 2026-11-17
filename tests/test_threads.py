"""Threads: the work done in Rust runs with the GIL released.

While another thread parses, validates, renders or saves a large table, the main
thread must keep running. Holding the GIL would stall it for the whole call.
"""

import functools
import json
import sys
import threading
import time
from concurrent.futures import ThreadPoolExecutor

import pytest

import gridwell

ROWS = 50_000
COLS = 6


def cell(text):
    return {"content": [{"type": "text", "value": text}]}


def big_ir(rows=ROWS):
    return {
        "ir_version": "1.0",
        "config": {"table_cols": COLS, "body_rows": rows, "header_rows": 1},
        "styles": {"defs": {}, "compositions": {}, "conditionals": []},
        "column_spec": [{"id": f"c{i}"} for i in range(COLS)],
        "table": {
            "thead": {"rows": [{"cells": [cell(f"H{i}") for i in range(COLS)]}]},
            "tbody": [
                {
                    "rows": [
                        {"cells": [cell(f"r{r} c{i}") for i in range(COLS)]} for r in range(rows)
                    ]
                }
            ],
        },
    }


@pytest.fixture(scope="module")
def big():
    source = json.dumps(big_ir())
    table = gridwell.Table(source)
    assert table.validate() == []
    return source, table


# Each takes the JSON source, the parsed table and a scratch directory.
OPERATIONS = {
    "Table(str)": lambda s, t, d: gridwell.Table(s),
    "Table.from_json": lambda s, t, d: gridwell.Table.from_json(s),
    "parse_ir": lambda s, t, d: gridwell.parse_ir(s),
    "Table(table)": lambda s, t, d: gridwell.Table(t),
    "validate": lambda s, t, d: t.validate(),
    "warnings": lambda s, t, d: t.warnings("latex"),
    "to_json": lambda s, t, d: t.to_json(),
    "render": lambda s, t, d: t.render("html"),
    "render_ansi": lambda s, t, d: t.render_ansi(),
    "render_binary": lambda s, t, d: t.render_binary("xlsx"),
    "render_docx": lambda s, t, d: t.render_docx(),
    "save": lambda s, t, d: t.save(d / "table.html"),
    "repr": lambda s, t, d: repr(t),
    "str": lambda s, t, d: str(t),
    "_repr_html_": lambda s, t, d: t._repr_html_(),
}


def result_share(result, cpu_per_call):
    """The share of a call that holds the GIL whatever the binding does: making the
    Python `str` or `bytes` of a result (a copy out of Rust) and freeing it, or
    freeing a `Table` result (its Rust drop runs in the deallocator). Small beside
    the work for most calls, but `to_json`'s result is 157 MB, and where each copy
    faults in fresh pages (CI's Linux runners) it takes about a third of the call,
    and freeing the table takes about a fifth of a `Table(table)` call. At most 0.3,
    so a call that held the GIL throughout still fails."""
    if isinstance(result, str):
        step = result.encode().decode
    elif isinstance(result, bytes):
        step = functools.partial(bytearray, result)
    elif isinstance(result, gridwell.Table):
        copies = [gridwell.Table(result) for _ in range(3)]
        step = copies.pop  # the popped copy is freed as soon as it's returned
    else:
        return 0.0
    times = []
    for _ in range(3):
        start = time.thread_time()
        step()
        times.append(time.thread_time() - start)
    return min(0.3, sorted(times)[1] / cpu_per_call)


def measure(operation, source, table, tmp_path):
    """Run `operation` over and over on a worker thread while the main thread spins.
    Returns the main thread's CPU time and the worker's, and the share of each call
    that goes to its result.

    CPU time, not wall time, so that a busy machine doesn't count: while the worker
    holds the GIL the main thread is blocked and gets no CPU at all, and while the
    worker is in Rust the two compete for the CPU on equal terms. So the main
    thread's CPU time over the worker's is the share of each call run with the GIL
    released, however many other processes there are."""
    done = threading.Event()
    calls = []
    worker_cpu = []

    def work():
        # Repeat for long enough that a stray scheduling delay doesn't count.
        start = time.perf_counter()
        cpu = time.thread_time()
        while True:
            operation(source, table, tmp_path)
            calls.append(time.perf_counter())
            if calls[-1] - start > 0.25:
                break
        worker_cpu.append(time.thread_time() - cpu)
        done.set()

    interval = sys.getswitchinterval()
    sys.setswitchinterval(0.001)
    try:
        worker = threading.Thread(target=work)
        cpu = time.thread_time()
        worker.start()
        while not done.is_set():
            pass
        main_cpu = time.thread_time() - cpu
        worker.join()
    finally:
        sys.setswitchinterval(interval)
    share = result_share(operation(source, table, tmp_path), worker_cpu[0] / len(calls))
    return main_cpu, worker_cpu[0], share


@pytest.mark.parametrize("name", sorted(OPERATIONS))
def test_other_threads_run_meanwhile(big, tmp_path, name):
    # With the GIL held, the main thread would get no CPU during the calls: a call
    # that holds it scores about 0.02, and one that releases it about 0.9 (above
    # 0.55 with twice as many busy processes as CPUs). Nothing lets the main thread
    # run while the worker holds the GIL, so one good run of three is enough, and a
    # call that holds the GIL fails all three. (On a slow runner a big call is a
    # single sample, and one hiccup can sink it.)
    source, table = big
    runs = []
    for _ in range(3):
        main_cpu, worker_cpu, share = measure(OPERATIONS[name], source, table, tmp_path)
        if main_cpu / worker_cpu > 0.5 - share:
            return
        runs.append(
            f"the main thread ran {main_cpu:.3f}s of the worker's {worker_cpu:.3f}s "
            f"({share:.2f} of each call goes to its result)"
        )
    pytest.fail(f"{name}: " + "; ".join(runs))


def test_one_table_renders_alike_on_many_threads():
    table = gridwell.Table(big_ir(rows=2_000))
    jobs = [table.render_html, table.render_latex, table.render_xlsx, table.to_json] * 4
    alone = [job() for job in jobs]
    with ThreadPoolExecutor(max_workers=8) as pool:
        together = list(pool.map(lambda job: job(), jobs))
    assert together == alone
