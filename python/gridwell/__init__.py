"""gridwell: Fast multi-format table rendering from a declarative IR."""

from gridwell._native import InvalidOptionsError, InvalidTableError, Table, formats, parse_ir

__all__ = [
    "InvalidOptionsError",
    "InvalidTableError",
    "Table",
    "formats",
    "parse_ir",
    "gt_to_ir",
    "gt_to_dict",
]
__version__ = "0.1.0"

# The Great Tables emitter needs pandas and great_tables, which are optional (the `gt`
# extra). Load it on first use so `import gridwell` works without them.
_GT_EMITTER_NAMES = frozenset({"gt_to_ir", "gt_to_dict"})


def __getattr__(name: str):
    if name in _GT_EMITTER_NAMES:
        try:
            from gridwell import _gt_emitter
        except ImportError as e:
            raise ImportError(
                f"gridwell.{name} needs the optional Great Tables dependencies; "
                "install them with: pip install 'gridwell[gt]'"
            ) from e
        return getattr(_gt_emitter, name)
    raise AttributeError(f"module 'gridwell' has no attribute {name!r}")


def __dir__():
    return sorted(set(globals()) | _GT_EMITTER_NAMES)
