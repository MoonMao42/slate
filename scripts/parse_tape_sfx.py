#!/usr/bin/env python3
"""Parse a VHS .tape file and emit SFX timing markers.

Usage: parse_tape_sfx.py <tape-file>
Output: one line per marker, ``<EVENT_NAME> <OFFSET_SECONDS>``.

VHS directive timing model :
  * ``Sleep Nms`` / ``Sleep Ns``  advances the clock by that duration.
  * ``Type "..."`` advances by ``len(text) * typing_speed`` seconds. The
    typing speed defaults to 100 ms/char and is overridden by
    ``Set TypingSpeed Nms``.
  * ``Enter`` / ``Tab`` / ``Up`` / ``Down`` / ``Left`` / ``Right`` /
    ``Escape`` cost roughly 50 ms of key-press overhead.
  * ``Set`` / ``Output`` / ``Env`` directives do not advance the clock.

Marker convention:
  A trailing ``# SFX: <EventName>`` comment on the SAME line as a
  directive attaches to THAT directive's start time (i.e. the clock value
  just before the directive advances it). A marker on its own line
  attaches to the PREVIOUS directive's start time. The event name is
  expected to match an entry in scripts/bake-audio.sh's SFX_WAV map.
"""
from __future__ import annotations

import re
import sys
from pathlib import Path

_SFX_RE = re.compile(r"#\s*SFX:\s*(\w+)")
_COMMENT_SPLIT_RE = re.compile(r"\s+#")
_TYPING_SPEED_RE = re.compile(r"Set\s+TypingSpeed\s+(\d+)ms")
_SLEEP_RE = re.compile(r"Sleep\s+(\d+)(ms|s)")
_TYPE_RE = re.compile(r'Type\s+"(.*)"')

_KEY_DIRECTIVES = {"Enter", "Tab", "Up", "Down", "Left", "Right", "Escape"}
_KEY_PRESS_COST = 0.05  # seconds

_DEFAULT_TYPING_SPEED = 0.1  # seconds per character (VHS default)


def parse(tape_path: str | Path) -> list[tuple[str, float]]:
    """Return ``[(event_name, offset_seconds), ...]`` for the given .tape."""
    markers: list[tuple[str, float]] = []
    clock = 0.0
    typing_speed = _DEFAULT_TYPING_SPEED
    last_directive_start = 0.0

    for raw in Path(tape_path).read_text().splitlines():
        line = raw.rstrip()
        stripped = line.strip()

        inline_marker = _SFX_RE.search(line)
        directive = _COMMENT_SPLIT_RE.split(stripped, maxsplit=1)[0].strip()

        # Blank / pure-comment lines don't advance the clock, but a marker
        # on its own line still attaches to the previous directive.
        if not directive or directive.startswith("#"):
            if inline_marker and not directive:
                markers.append((inline_marker.group(1), last_directive_start))
            continue

        # Record the start time BEFORE the directive advances the clock.
        last_directive_start = clock

        if m := _TYPING_SPEED_RE.match(directive):
            typing_speed = int(m.group(1)) / 1000.0
        elif m := _SLEEP_RE.match(directive):
            n = int(m.group(1))
            clock += (n / 1000.0) if m.group(2) == "ms" else float(n)
        elif m := _TYPE_RE.match(directive):
            clock += len(m.group(1)) * typing_speed
        elif directive in _KEY_DIRECTIVES:
            clock += _KEY_PRESS_COST
        # Set / Output / Env lines don't advance the clock.

        if inline_marker:
            markers.append((inline_marker.group(1), last_directive_start))

    return markers


def main() -> int:
    if len(sys.argv) != 2:
        print("usage: parse_tape_sfx.py <tape-file>", file=sys.stderr)
        return 2
    try:
        markers = parse(sys.argv[1])
    except FileNotFoundError as exc:
        print(f"parse_tape_sfx.py: {exc}", file=sys.stderr)
        return 1
    for event, offset in markers:
        print(f"{event} {offset:.3f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
