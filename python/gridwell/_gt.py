"""Convert a Great Tables `GT` to gridwell IR.

The emitter is moving into Great Tables, as `GT.as_ir()`. When the installed Great
Tables has it, it is used; otherwise gridwell's own copy (`_gt_emitter`) is.
"""

from __future__ import annotations

import importlib.metadata
import json
import re
import warnings
from typing import Any, Literal

from great_tables import GT

# The IR major version this gridwell reads.
IR_MAJOR = 1

Emitter = Literal["auto", "upstream", "gridwell"]
EMITTERS = ("auto", "upstream", "gridwell")


def gt_to_ir(gt_obj: GT, emitter: Emitter = "auto") -> str:
    """Convert a Great Tables GT object to gridwell IR JSON.

    Parameters
    ----------
    gt_obj : great_tables.GT
        A Great Tables GT object.
    emitter : {"auto", "upstream", "gridwell"}
        Which converter to use. "auto" uses Great Tables' own (`GT.as_ir()`) when the
        installed Great Tables has one, and gridwell's otherwise, or when Great
        Tables' fails or gives an IR version this gridwell can't read (with a
        warning saying why). "upstream" uses Great Tables' and raises if it can't;
        "gridwell" uses gridwell's.

    Returns
    -------
    str
        JSON string in gridwell IR format.
    """
    upstream = _upstream(gt_obj, emitter)
    if upstream is not None:
        return upstream[0]
    return json.dumps(_own().build_ir(gt_obj), indent=2)


def gt_to_dict(gt_obj: GT, emitter: Emitter = "auto") -> dict[str, Any]:
    """Convert a Great Tables GT object to gridwell IR dict.

    Parameters
    ----------
    gt_obj : great_tables.GT
        A Great Tables GT object.
    emitter : {"auto", "upstream", "gridwell"}
        Which converter to use, as for `gt_to_ir`.

    Returns
    -------
    dict
        Dictionary in gridwell IR format.
    """
    upstream = _upstream(gt_obj, emitter)
    if upstream is not None:
        return upstream[1]
    return _own().build_ir(gt_obj)


def has_upstream(gt_obj: GT) -> bool:
    """Whether Great Tables converts this table itself (`GT.as_ir()`)."""
    return callable(getattr(type(gt_obj), "as_ir", None))


def _upstream(gt_obj: GT, emitter: str) -> tuple[str, dict[str, Any]] | None:
    """Great Tables' IR, as JSON and as a dict, or `None` to use gridwell's own."""
    if emitter not in EMITTERS:
        raise ValueError(f"emitter must be one of {', '.join(map(repr, EMITTERS))}; got {emitter!r}")
    if not isinstance(gt_obj, GT):
        raise TypeError(f"gt_obj must be a great_tables.GT; got {type(gt_obj).__name__}")
    if emitter == "gridwell":
        return None
    if not has_upstream(gt_obj):
        if emitter == "upstream":
            raise RuntimeError(
                f"Great Tables {_version()} has no GT.as_ir(); use emitter='gridwell'"
            )
        return None
    try:
        text = gt_obj.as_ir()  # type: ignore[attr-defined]
        return text, _checked(text)
    except Exception as e:
        if emitter == "upstream":
            raise
        warnings.warn(
            f"{e}; converted with gridwell's own emitter instead",
            UserWarning,
            stacklevel=3,
        )
        return None


def _checked(text: Any) -> dict[str, Any]:
    """The IR `GT.as_ir()` gave, parsed, if it's IR this gridwell reads."""
    if not isinstance(text, str):
        raise ValueError(f"GT.as_ir() gave a {type(text).__name__}, not a JSON string")
    try:
        ir = json.loads(text)
    except ValueError as e:
        raise ValueError(f"GT.as_ir() gave JSON that doesn't parse ({e})") from None
    version = ir.get("ir_version") if isinstance(ir, dict) else None
    found = re.fullmatch(r"([0-9]+)\.[0-9]+", version) if isinstance(version, str) else None
    if found is None:
        raise ValueError(f"GT.as_ir() gave an ir_version of {version!r}, not MAJOR.MINOR")
    if int(found[1]) != IR_MAJOR:
        raise ValueError(
            f"GT.as_ir() gives IR {version}, and this gridwell reads IR {IR_MAJOR}.x "
            "(a newer gridwell may read it)"
        )
    return ir


def _own() -> Any:
    """gridwell's own emitter, which needs pandas."""
    try:
        from gridwell import _gt_emitter
    except ImportError as e:
        raise ImportError(
            "gridwell's Great Tables emitter needs the optional dependencies; "
            "install them with: pip install 'gridwell[gt]'"
        ) from e
    return _gt_emitter


def _version() -> str:
    try:
        return importlib.metadata.version("great_tables")
    except importlib.metadata.PackageNotFoundError:
        return "(unknown version)"
