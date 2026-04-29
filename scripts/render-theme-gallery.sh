#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
THEMES_TOML="${ROOT_DIR}/themes/themes.toml"

python3 - "$THEMES_TOML" <<'PY'
import html
import sys

try:
    import tomllib
except ModuleNotFoundError:
    import tomli as tomllib

with open(sys.argv[1], "rb") as fh:
    data = tomllib.load(fh)

print("| Family | Variant | ID | Appearance | Palette |")
print("|--------|---------|----|-----------:|---------|")

for theme in data["theme"]:
    palette = theme["palette"]
    swatches = [
        palette["background"],
        palette["foreground"],
        palette["brand_accent"],
        palette["red"],
    ]
    rects = "".join(
        f'<rect width="20" height="14" x="{idx * 20}" fill="{color}"/>'
        for idx, color in enumerate(swatches)
    )
    svg = f'<svg width="80" height="14" xmlns="http://www.w3.org/2000/svg">{rects}</svg>'
    print(
        "| {family} | {name} | `{theme_id}` | {appearance} | {svg} |".format(
            family=html.escape(theme["family"]),
            name=html.escape(theme["name"]),
            theme_id=html.escape(theme["id"]),
            appearance=html.escape(theme["appearance"]),
            svg=svg,
        )
    )
PY
