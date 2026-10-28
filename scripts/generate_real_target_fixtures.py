#!/usr/bin/env python3
"""Generate deterministic PNG fixtures and independent RGBA8 expectations."""

import argparse
import binascii
import hashlib
import json
import struct
import zlib
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


def chunk(kind, data, bad_crc=False):
    crc = binascii.crc32(kind + data) & 0xFFFFFFFF
    if bad_crc:
        crc ^= 1
    return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", crc)


def make_png(width, height, depth, color_type, rows, *, interlace=0, before_idat=()):
    ihdr = struct.pack(">IIBBBBB", width, height, depth, color_type, 0, 0, interlace)
    raw = b"".join(b"\x00" + row for row in rows)
    return PNG_SIGNATURE + chunk(b"IHDR", ihdr) + b"".join(before_idat) + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b"")


def adam7_rgba(width, height, pixels):
    starts = ((0, 0), (4, 0), (0, 4), (2, 0), (0, 2), (1, 0), (0, 1))
    steps = ((8, 8), (8, 8), (4, 8), (4, 4), (2, 4), (2, 2), (1, 2))
    raw = bytearray()
    for (start_x, start_y), (step_x, step_y) in zip(starts, steps):
        for y in range(start_y, height, step_y):
            row = bytearray()
            for x in range(start_x, width, step_x):
                offset = (y * width + x) * 4
                row.extend(pixels[offset : offset + 4])
            if row:
                raw.append(0)
                raw.extend(row)
    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 1)
    return PNG_SIGNATURE + chunk(b"IHDR", ihdr) + chunk(b"IDAT", zlib.compress(bytes(raw), 9)) + chunk(b"IEND", b"")


def canonical(width, height, rgba):
    return b"\x01" + struct.pack("<IIQ", width, height, len(rgba)) + rgba


def record(name, png, *, rgba=None, dimensions=None, expected=None):
    result = {"name": name, "path": f"seeds/{name}.png", "sha256": hashlib.sha256(png).hexdigest()}
    if rgba is not None:
        width, height = dimensions
        result.update({"matching": True, "expected_class": "accepted", "dimensions": [width, height], "canonical_sha256": hashlib.sha256(canonical(width, height, rgba)).hexdigest()})
    else:
        result["expected_by_target"] = expected
    return result


def generate(output):
    seeds = output / "seeds"
    seeds.mkdir(parents=True, exist_ok=True)
    fixtures = []

    def add(name, png, **expectation):
        (seeds / f"{name}.png").write_bytes(png)
        fixtures.append(record(name, png, **expectation))

    rgba = bytes([255, 0, 0, 255, 0, 255, 0, 255])
    add("rgb8", make_png(2, 1, 8, 2, [bytes([255, 0, 0, 0, 255, 0])]), rgba=rgba, dimensions=(2, 1))
    add("rgba8", make_png(2, 1, 8, 6, [rgba]), rgba=rgba, dimensions=(2, 1))
    palette_rgba = bytes([10, 20, 30, 0, 40, 50, 60, 200])
    add("palette-trns", make_png(2, 1, 8, 3, [bytes([0, 1])], before_idat=(chunk(b"PLTE", bytes([10, 20, 30, 40, 50, 60])), chunk(b"tRNS", bytes([0, 200])))), rgba=palette_rgba, dimensions=(2, 1))
    add("gray-low-bit", make_png(2, 1, 1, 0, [bytes([0b10000000])]), rgba=bytes([255, 255, 255, 255, 0, 0, 0, 255]), dimensions=(2, 1))
    add("gray-alpha", make_png(2, 1, 8, 4, [bytes([7, 9, 200, 201])]), rgba=bytes([7, 7, 7, 9, 200, 200, 200, 201]), dimensions=(2, 1))
    rgba16_raw = struct.pack(">8H", 0x1234, 0x5678, 0x9ABC, 0xDEF0, 0xFFFF, 0, 0x8001, 0x0102)
    add("rgba16", make_png(2, 1, 16, 6, [rgba16_raw]), rgba=bytes([0x12, 0x56, 0x9A, 0xDE, 0xFF, 0, 0x80, 0x01]), dimensions=(2, 1))
    adam_pixels = bytes([1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255])
    add("adam7", adam7_rgba(2, 2, adam_pixels), rgba=adam_pixels, dimensions=(2, 2))
    add("gamma-srgb", make_png(2, 1, 8, 6, [rgba], before_idat=(chunk(b"gAMA", struct.pack(">I", 45455)), chunk(b"sRGB", b"\x00"))), rgba=rgba, dimensions=(2, 1))

    valid = make_png(2, 1, 8, 6, [rgba])
    rejected = {"a": "rejected", "b": "rejected"}
    add("truncated", valid[:-7], expected=rejected)
    bad_crc = bytearray(valid)
    idat = bad_crc.index(b"IDAT")
    idat_length = struct.unpack(">I", bad_crc[idat - 4 : idat])[0]
    bad_crc[idat + 4 + idat_length + 3] ^= 1
    add("bad-crc", bytes(bad_crc), expected=rejected)
    add("missing-iend", valid[:-12], expected=rejected)
    add("bad-iend", valid[:-4] + bytes([valid[-4] ^ 1]) + valid[-3:], expected=rejected)
    add("apng", make_png(2, 1, 8, 6, [rgba], before_idat=(chunk(b"acTL", struct.pack(">II", 1, 0)),)), expected={"a": "unsupported", "b": "unsupported"})

    for name, width, resource in (("width-4096", 4096, False), ("width-4097", 4097, True)):
        row = bytes([0, 0, 0, 255]) * width
        png = make_png(width, 1, 8, 6, [row])
        add(name, png, **({"expected": {"a": "resource-limit", "b": "resource-limit"}} if resource else {"rgba": row, "dimensions": (width, 1)}))
    add("zero-width", make_png(0, 1, 8, 6, [b""]), expected=rejected)
    for name, width, height, resource in (("pixel-limit", 2048, 2048, False), ("pixel-over-limit", 2049, 2048, True)):
        row = bytes([0, 0, 0, 255]) * width
        png = make_png(width, height, 8, 6, [row] * height)
        add(name, png, **({"expected": {"a": "resource-limit", "b": "resource-limit"}} if resource else {"rgba": row * height, "dimensions": (width, height)}))

    (output / "fixtures.json").write_text(json.dumps({"schema_version": 1, "generator": "scripts/generate_real_target_fixtures.py", "fixtures": fixtures}, indent=2, sort_keys=True) + "\n")


def make_bmp(
    width,
    height,
    pixels,
    *,
    bits_per_pixel=24,
    compression=0,
    palette=b"",
    masks=b"",
):
    offset = 14 + 40 + len(masks) + len(palette)
    size = offset + len(pixels)
    return (
        b"BM"
        + struct.pack("<IHHI", size, 0, 0, offset)
        + struct.pack(
            "<IiiHHIIiiII",
            40,
            width,
            height,
            1,
            bits_per_pixel,
            compression,
            len(pixels),
            0,
            0,
            len(palette) // 4,
            0,
        )
        + masks
        + palette
        + pixels
    )


def make_bmp_v4(width, height, pixels, *, bits_per_pixel, compression, masks):
    dib = (
        struct.pack(
            "<IiiHHIIiiII",
            108,
            width,
            height,
            1,
            bits_per_pixel,
            compression,
            len(pixels),
            0,
            0,
            0,
            0,
        )
        + struct.pack("<IIII", *masks)
        + struct.pack("<I", 0x73524742)
        + bytes(48)
    )
    offset = 14 + len(dib)
    return b"BM" + struct.pack("<IHHI", offset + len(pixels), 0, 0, offset) + dib + pixels


def generate_bmp(output):
    seeds = output / "seeds"
    seeds.mkdir(parents=True, exist_ok=True)
    fixtures = []

    def add(name, payload, *, rgba=None, dimensions=None, expected=None):
        path = seeds / f"{name}.bmp"
        path.write_bytes(payload)
        fixture = {
            "name": name,
            "path": f"seeds/{name}.bmp",
            "sha256": hashlib.sha256(payload).hexdigest(),
        }
        if rgba is not None:
            width, height = dimensions
            fixture.update(
                {
                    "matching": True,
                    "expected_class": "accepted",
                    "dimensions": [width, height],
                    "canonical_sha256": hashlib.sha256(
                        canonical(width, height, rgba)
                    ).hexdigest(),
                }
            )
        else:
            fixture["expected_by_target"] = expected
        fixtures.append(fixture)

    palette = bytes(
        [
            30,
            20,
            10,
            0,
            60,
            50,
            40,
            0,
            90,
            80,
            70,
            0,
        ]
    )
    colors = (
        bytes([10, 20, 30, 255]),
        bytes([40, 50, 60, 255]),
        bytes([70, 80, 90, 255]),
    )
    add(
        "rgb24-2x1",
        make_bmp(2, 1, bytes([0, 0, 255, 0, 255, 0, 0, 0])),
        rgba=bytes([255, 0, 0, 255, 0, 255, 0, 255]),
        dimensions=(2, 1),
    )
    add(
        "indexed1",
        make_bmp(3, 1, bytes([0x40, 0, 0, 0]), bits_per_pixel=1, palette=palette[:8]),
        rgba=colors[0] + colors[1] + colors[0],
        dimensions=(3, 1),
    )
    add(
        "indexed4",
        make_bmp(3, 1, bytes([0x01, 0x20, 0, 0]), bits_per_pixel=4, palette=palette),
        rgba=colors[0] + colors[1] + colors[2],
        dimensions=(3, 1),
    )
    add(
        "indexed8",
        make_bmp(3, 1, bytes([0, 1, 2, 0]), bits_per_pixel=8, palette=palette),
        rgba=colors[0] + colors[1] + colors[2],
        dimensions=(3, 1),
    )

    masks_565 = struct.pack("<III", 0xF800, 0x07E0, 0x001F)
    add(
        "rgb16-bitfields",
        make_bmp(
            2,
            1,
            struct.pack("<HH", 0xF800, 0x07E0),
            bits_per_pixel=16,
            compression=3,
            masks=masks_565,
        ),
        rgba=bytes([255, 0, 0, 255, 0, 255, 0, 255]),
        dimensions=(2, 1),
    )

    top = bytes([0, 0, 255, 0, 255, 0, 255, 0, 0, 0, 0, 0])
    bottom = bytes([255, 255, 0, 255, 0, 255, 0, 255, 255, 0, 0, 0])
    rgba_rows = bytes(
        [
            255,
            0,
            0,
            255,
            0,
            255,
            0,
            255,
            0,
            0,
            255,
            255,
            0,
            255,
            255,
            255,
            255,
            0,
            255,
            255,
            255,
            255,
            0,
            255,
        ]
    )
    add(
        "rgb24-bottom-up-odd",
        make_bmp(3, 2, bottom + top),
        rgba=rgba_rows,
        dimensions=(3, 2),
    )
    add(
        "rgb24-top-down-odd",
        make_bmp(3, -2, top + bottom),
        rgba=rgba_rows,
        dimensions=(3, 2),
    )
    add(
        "rgb32-unused-alpha",
        make_bmp(2, 1, bytes([3, 2, 1, 0, 30, 20, 10, 7]), bits_per_pixel=32),
        rgba=bytes([1, 2, 3, 255, 10, 20, 30, 255]),
        dimensions=(2, 1),
    )
    add(
        "rgba32-bitfields",
        make_bmp_v4(
            1,
            1,
            struct.pack("<I", 0x80112233),
            bits_per_pixel=32,
            compression=3,
            masks=(0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000),
        ),
        rgba=bytes([0x11, 0x22, 0x33, 0x80]),
        dimensions=(1, 1),
    )

    add(
        "rle8",
        make_bmp(
            3,
            1,
            bytes([1, 0, 1, 1, 1, 2, 0, 0, 0, 1]),
            bits_per_pixel=8,
            compression=1,
            palette=palette,
        ),
        rgba=colors[0] + colors[1] + colors[2],
        dimensions=(3, 1),
    )
    add(
        "rle4",
        make_bmp(
            3,
            1,
            bytes([3, 0x12, 0, 0, 0, 1]),
            bits_per_pixel=4,
            compression=2,
            palette=palette,
        ),
        rgba=colors[1] + colors[2] + colors[1],
        dimensions=(3, 1),
    )

    rejected = {"a": "rejected", "b": "rejected"}
    unsupported = {"a": "unsupported", "b": "unsupported"}
    resource = {"a": "resource-limit", "b": "resource-limit"}
    valid = make_bmp(1, 1, bytes([0, 0, 255, 0]))
    add("truncated-pixels", valid[:-1], expected=rejected)
    indexed = make_bmp(1, 1, bytes([0, 0, 0, 0]), bits_per_pixel=8, palette=palette[:4])
    add("truncated-palette", indexed[:56], expected=rejected)
    bad_offset = bytearray(valid)
    bad_offset[10:14] = struct.pack("<I", 13)
    add("bad-offset", bytes(bad_offset), expected=rejected)
    add("second-image", valid + valid, expected=unsupported)
    embedded = bytearray(valid)
    embedded[30:34] = struct.pack("<I", 5)
    add("embedded-png", bytes(embedded), expected=unsupported)
    add("width-4097", make_bmp(4097, 1, b""), expected=resource)
    add("pixel-over-limit", make_bmp(2049, 2048, b""), expected=resource)

    (output / "fixtures.json").write_text(
        json.dumps(
            {
                "schema_version": 1,
                "generator": "scripts/generate_real_target_fixtures.py",
                "fixtures": fixtures,
            },
            indent=2,
            sort_keys=True,
        )
        + "\n"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pair", choices=("png", "bmp"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.pair == "png":
        generate(arguments.output)
    else:
        generate_bmp(arguments.output)


if __name__ == "__main__":
    main()
