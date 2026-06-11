"""gridwell: Fast multi-format table rendering from a declarative IR."""

from gridwell._native import Table, parse_ir
from gridwell._gt_emitter import gt_to_ir, gt_to_dict

__all__ = ["Table", "parse_ir", "gt_to_ir", "gt_to_dict"]
__version__ = "0.1.0"
