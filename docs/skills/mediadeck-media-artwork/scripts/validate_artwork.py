#!/usr/bin/env python3
"""Validate MediaDeck cover/label PNG dimensions without third-party modules."""

from __future__ import annotations

import argparse
import struct
import sys
from pathlib import Path


PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"
COVER_SIZE = (1518, 2076)
PRESETS = {
    "floppy_35_label": (827, 614),
    "cd_jewel_front": (1417, 1417),
    "dvd_case_front": (1535, 2161),
}
TARGET_PIXELS_PER_METRE = 11811


class ValidationError(Exception):
    """Raised when an artwork file violates the output contract."""


def read_png(path: Path) -> tuple[int, int, tuple[int, int, int] | None]:
    with path.open("rb") as source:
        if source.read(8) != PNG_SIGNATURE:
            raise ValidationError(f"{path}: expected a PNG file")

        width = height = None
        density = None
        while True:
            length_raw = source.read(4)
            if not length_raw:
                break
            if len(length_raw) != 4:
                raise ValidationError(f"{path}: truncated PNG chunk")
            length = struct.unpack(">I", length_raw)[0]
            chunk_type = source.read(4)
            data = source.read(length)
            crc = source.read(4)
            if len(chunk_type) != 4 or len(data) != length or len(crc) != 4:
                raise ValidationError(f"{path}: truncated PNG chunk")
            if chunk_type == b"IHDR":
                width, height = struct.unpack(">II", data[:8])
            elif chunk_type == b"pHYs" and len(data) == 9:
                density = struct.unpack(">IIB", data)
            elif chunk_type == b"IEND":
                break

    if width is None or height is None:
        raise ValidationError(f"{path}: missing IHDR")
    return width, height, density


def validate_file(
    path: Path,
    expected: tuple[int, int],
    require_density: bool,
) -> list[str]:
    if not path.is_file():
        raise ValidationError(f"{path}: file does not exist")

    width, height, density = read_png(path)
    if (width, height) != expected:
        raise ValidationError(
            f"{path}: expected {expected[0]}x{expected[1]}, got {width}x{height}"
        )

    warnings: list[str] = []
    if require_density:
        if density is None:
            raise ValidationError(f"{path}: missing PNG pHYs density metadata")
        x_ppm, y_ppm, unit = density
        if unit != 1:
            raise ValidationError(f"{path}: PNG density unit is not metres")
        if abs(x_ppm - TARGET_PIXELS_PER_METRE) > 2 or abs(
            y_ppm - TARGET_PIXELS_PER_METRE
        ) > 2:
            raise ValidationError(
                f"{path}: expected about 300 PPI/11811 ppm, got {x_ppm}x{y_ppm} ppm"
            )
    elif density is None:
        warnings.append(f"{path}: no density metadata; set 300 PPI before printing")

    return warnings


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Validate a MediaDeck launcher cover and printable artwork pair."
    )
    parser.add_argument("--cover", required=True, type=Path)
    parser.add_argument("--label", required=True, type=Path)
    parser.add_argument("--preset", required=True, choices=sorted(PRESETS))
    parser.add_argument(
        "--strict-density",
        action="store_true",
        help="Require 300 PPI pHYs metadata on the printable artwork.",
    )
    args = parser.parse_args()

    try:
        warnings = validate_file(args.cover, COVER_SIZE, False)
        warnings.extend(
            validate_file(
                args.label,
                PRESETS[args.preset],
                args.strict_density,
            )
        )
    except ValidationError as error:
        print(f"ERROR: {error}", file=sys.stderr)
        return 1

    print(f"OK: cover {COVER_SIZE[0]}x{COVER_SIZE[1]}")
    label_size = PRESETS[args.preset]
    print(f"OK: {args.preset} {label_size[0]}x{label_size[1]}")
    for warning in warnings:
        print(f"WARNING: {warning}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
