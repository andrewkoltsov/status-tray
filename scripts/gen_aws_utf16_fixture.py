#!/usr/bin/env python3
"""Regenerate crates/status-core/tests/fixtures/aws_currentevents_utf16.bin.

The real endpoint serves UTF-16 JSON; this re-encodes the (UTF-8) JSON fixture as UTF-16LE with
BOM, compact, so the decoder test is reproducible. Standard library only.
"""
import json
from pathlib import Path

fx = Path(__file__).resolve().parent.parent / "crates/status-core/tests/fixtures"
data = json.loads((fx / "aws_currentevents.json").read_text(encoding="utf-8"))
text = json.dumps(data, separators=(",", ":"), ensure_ascii=False)
(fx / "aws_currentevents_utf16.bin").write_bytes(b"\xff\xfe" + text.encode("utf-16-le"))
