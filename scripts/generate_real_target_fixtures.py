#!/usr/bin/env python3
"""Generate deterministic PNG fixtures and independent RGBA8 expectations."""

import argparse
import base64
import binascii
import hashlib
import json
import struct
import zlib
from pathlib import Path

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"

X509_PROBE_DER_BASE64 = """
MIICMTCCAZqgAwIBAgIBATANBgkqhkiG9w0BAQsFADA0MRswGQYDVQQDDBJEaWZmUmF5IFg1MDkg
UHJvYmUxFTATBgNVBAoMDERpZmZSYXkgVGVzdDAeFw0yNjEwMjgwNzA1NDJaFw0zNjEwMjUwNzA1
NDJaMDQxGzAZBgNVBAMMEkRpZmZSYXkgWDUwOSBQcm9iZTEVMBMGA1UECgwMRGlmZlJheSBUZXN0
MIGfMA0GCSqGSIb3DQEBAQUAA4GNADCBiQKBgQDZ7EwSm1uzbUIXTpNptBiV0ak5nHSVutdVhc68
zrVkEJnElyMxTb4Xb4ZyTfpcScSLHfVs5qKb15qysKkx2NZ7wC6y5FKKAlDXkvmiHLly7gHkVibP
Tr2D0XjR8Vik0LCSwOhqfYzlPCCsQ71/k42dlg559ajhI37h3YmCrPT/GQIDAQABo1MwUTAdBgNV
HQ4EFgQU2aJ/pQzWp6g4HzggnXE2LxzUIYswHwYDVR0jBBgwFoAU2aJ/pQzWp6g4HzggnXE2LxzU
IYswDwYDVR0TAQH/BAUwAwEB/zANBgkqhkiG9w0BAQsFAAOBgQCDUi0Fw71znzLdXuHyqo1V3VKx
kgxwES3d8jPNuGILVATRJ83sI86M49ZbHzP5iO+FeUE3bJ9m6Ml1BvEQf9pGSjFBcRt0mpStT/Nn
fmFEDSAkqT8Ngkd1/pS/rynVSLPYUnebnYNqu1GFHiNJa2gUbQd8uKQL5BStNPsxolLiRg==
"""

X509_RSA_PRIVATE_KEY_PEM = b"""-----BEGIN PRIVATE KEY-----
MIICdgIBADANBgkqhkiG9w0BAQEFAASCAmAwggJcAgEAAoGBANnsTBKbW7NtQhdO
k2m0GJXRqTmcdJW611WFzrzOtWQQmcSXIzFNvhdvhnJN+lxJxIsd9WzmopvXmrKw
qTHY1nvALrLkUooCUNeS+aIcuXLuAeRWJs9OvYPReNHxWKTQsJLA6Gp9jOU8IKxD
vX+TjZ2WDnn1qOEjfuHdiYKs9P8ZAgMBAAECgYB6yusSkPfa4nWbLGtj5Fb2FG2G
SOJofB9Z0Uvsl40eFTyPyZnK7kNucSkQv1+TIP3eZcMk2dBGf/ZrOaActlIj3j1D
V84dnU5TcVDPTIVVNyiI7pSeMFT2UPZZzDNuTU+kZXBTYMsHrK4kq2eOpON8PLLg
WVlLSjw/3Z4KxgbgAQJBAPrOlqXc4zEzC7QWXuKanPAMoR1mEN9lf2++JR4wgpT8
3ERXc7t0AndGQbrNRdUm8bm2/Xm6hBDaryPoyQdTQLECQQDeb2f3WcYERynum9Op
VvLa7GNzHPCJKn6SqtmeRJEliFT0NvKlfcNTV9sNEYrZj7QRQYt7+aUlOTPgdr1j
2H7pAkBk0ywInZXPr3+i8+NNIW65qboTQXbC9+CVIVF2o3W7QzOhXa6uyuI65MmD
uQW7vJ4sVH3ts6dnxyHlQ62c/kDhAkAEeE6pjNdK3Lc+AUmmnIWy2o9BD/LRftYe
UmqfIJeeHng+lCAaLUXQlfJj99dHefjPY/bOnfywm86sExbJO9+JAkEAq5nvfViu
PL+NAR2DX3AO2KriacQOtPJuaWZNeGe8Obpx6tQ2ri1zzOMWd6I8hfVTPQHbTpFt
Q0tc3R1K1ugJvQ==
-----END PRIVATE KEY-----
"""


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


def generate_x509(output):
    from datetime import datetime, timezone

    from cryptography import x509
    from cryptography.hazmat.primitives import hashes, serialization
    from cryptography.hazmat.primitives.asymmetric import ec
    from cryptography.x509.oid import NameOID, ObjectIdentifier

    seeds = output / "seeds"
    seeds.mkdir(parents=True, exist_ok=True)
    fixtures = []
    rejected = {"a": "rejected", "b": "rejected"}
    resource = {"a": "resource-limit", "b": "resource-limit"}

    def der_length(length):
        if length < 0x80:
            return bytes([length])
        encoded = length.to_bytes((length.bit_length() + 7) // 8, "big")
        return bytes([0x80 | len(encoded)]) + encoded

    def tlv(payload, tag=0x30):
        return bytes([tag]) + der_length(len(payload)) + payload

    def read_tlv(payload, offset=0):
        tag = payload[offset]
        first = payload[offset + 1]
        if first < 0x80:
            header = 2
            length = first
        else:
            count = first & 0x7F
            header = 2 + count
            length = int.from_bytes(payload[offset + 2 : offset + header], "big")
        end = offset + header + length
        return tag, offset + header, end

    def canonical_der(der):
        return b"\x02" + struct.pack("<Q", len(der)) + der

    def add_matching(name, der):
        path = seeds / f"{name}.der"
        path.write_bytes(der)
        fixtures.append(
            {
                "name": name,
                "path": f"seeds/{name}.der",
                "sha256": hashlib.sha256(der).hexdigest(),
                "matching": True,
                "expected_class": "accepted",
                "canonical_sha256": hashlib.sha256(canonical_der(der)).hexdigest(),
            }
        )

    def add_invalid(name, payload, expected=rejected, suffix="der"):
        path = seeds / f"{name}.{suffix}"
        path.write_bytes(payload)
        fixtures.append(
            {
                "name": name,
                "path": f"seeds/{name}.{suffix}",
                "sha256": hashlib.sha256(payload).hexdigest(),
                "expected_by_target": expected,
            }
        )

    key = serialization.load_pem_private_key(X509_RSA_PRIVATE_KEY_PEM, None)
    base_name = x509.Name(
        [
            x509.NameAttribute(NameOID.COMMON_NAME, "DiffRay deterministic X509"),
            x509.NameAttribute(NameOID.ORGANIZATION_NAME, "DiffRay Test"),
        ]
    )

    def certificate(
        name,
        *,
        serial,
        subject=None,
        public_key=None,
        not_before=datetime(2024, 1, 2, 3, 4, 5, tzinfo=timezone.utc),
        not_after=datetime(2034, 1, 2, 3, 4, 5, tzinfo=timezone.utc),
        extensions=(),
    ):
        builder = (
            x509.CertificateBuilder()
            .subject_name(subject or base_name)
            .issuer_name(base_name)
            .public_key(public_key or key.public_key())
            .serial_number(serial)
            .not_valid_before(not_before)
            .not_valid_after(not_after)
        )
        for extension, critical in extensions:
            builder = builder.add_extension(extension, critical)
        der = builder.sign(key, hashes.SHA256()).public_bytes(serialization.Encoding.DER)
        add_matching(name, der)
        return der

    rsa_v3 = certificate(
        "rsa-v3",
        serial=1,
        extensions=((x509.BasicConstraints(ca=False, path_length=None), True),),
    )
    certificate(
        "ec-public-key",
        serial=2,
        public_key=ec.derive_private_key(7, ec.SECP256R1()).public_key(),
    )
    certificate("long-serial", serial=(1 << 158) + 0x12345)
    multi_name = x509.Name(
        [
            x509.NameAttribute(NameOID.COMMON_NAME, "DiffRay quoted + name"),
            x509.NameAttribute(NameOID.ORGANIZATIONAL_UNIT_NAME, "Parser Unit"),
            x509.NameAttribute(NameOID.ORGANIZATION_NAME, "DiffRay Test"),
        ]
    )
    certificate("multi-valued-name", serial=4, subject=multi_name)
    certificate(
        "time-variants",
        serial=5,
        not_before=datetime(2048, 12, 31, 23, 59, 58, tzinfo=timezone.utc),
        not_after=datetime(2051, 1, 1, 0, 0, 1, tzinfo=timezone.utc),
    )
    certificate(
        "unknown-oid",
        serial=6,
        extensions=(
            (
                x509.UnrecognizedExtension(
                    ObjectIdentifier("1.3.6.1.4.1.55555.1"), b"\x05\x00"
                ),
                False,
            ),
        ),
    )
    certificate(
        "dns-san",
        serial=7,
        extensions=((x509.SubjectAlternativeName([x509.DNSName("diffray.invalid")]), False),),
    )
    certificate(
        "unicode-name",
        serial=8,
        subject=x509.Name(
            [
                x509.NameAttribute(NameOID.COMMON_NAME, "DiffRay Unicode"),
                x509.NameAttribute(NameOID.ORGANIZATIONAL_UNIT_NAME, "解析器"),
                x509.NameAttribute(NameOID.ORGANIZATION_NAME, "DiffRay Test"),
            ]
        ),
    )
    certificate(
        "embedded-nul-name",
        serial=9,
        subject=x509.Name(
            [
                x509.NameAttribute(NameOID.COMMON_NAME, "DiffRay\x00NUL"),
                x509.NameAttribute(NameOID.ORGANIZATIONAL_UNIT_NAME, "Parser"),
                x509.NameAttribute(NameOID.ORGANIZATION_NAME, "DiffRay Test"),
            ]
        ),
    )

    # A certificate without the optional [0] EXPLICIT version is DER v1. The
    # signature is intentionally not verified by this parse/re-encode target.
    outer_tag, outer_start, outer_end = read_tlv(rsa_v3)
    tbs_tag, tbs_start, tbs_end = read_tlv(rsa_v3, outer_start)
    assert outer_tag == tbs_tag == 0x30 and rsa_v3[tbs_start : tbs_start + 5] == b"\xa0\x03\x02\x01\x02"
    v1_tbs = tlv(rsa_v3[tbs_start + 5 : tbs_end])
    rsa_v1 = tlv(v1_tbs + rsa_v3[tbs_end:outer_end])
    add_invalid("rsa-v1", rsa_v1, expected={"a": "accepted", "b": "rejected"})

    add_invalid("trailing-byte", rsa_v3 + b"\x00")
    add_invalid("truncated-tbs", rsa_v3[:tbs_end - 1])
    add_invalid("truncated-signature", rsa_v3[:-9])
    tbs_children = []
    cursor = tbs_start
    while cursor < tbs_end:
        tag, value_start, child_end = read_tlv(rsa_v3, cursor)
        tbs_children.append((tag, cursor, value_start, child_end))
        cursor = child_end
    for label, child in zip(
        ("version", "serial", "algorithm", "issuer", "validity", "subject", "spki", "extensions"),
        tbs_children,
    ):
        _, child_start, value_start, child_end = child
        cut = max(value_start, child_start + (child_end - child_start) // 2)
        add_invalid(f"truncated-{label}", rsa_v3[:cut])
    add_invalid("non-certificate", tlv(b"\x02\x01\x01"))
    pem = b"-----BEGIN CERTIFICATE-----\n" + base64.encodebytes(rsa_v3) + b"-----END CERTIFICATE-----\n"
    add_invalid("pem", pem, suffix="pem")
    add_invalid(
        "indefinite-length",
        b"\x30\x80" + rsa_v3[outer_start:outer_end] + b"\x00\x00",
        expected={"a": "accepted", "b": "rejected"},
    )
    outer_body = rsa_v3[outer_start:outer_end]
    add_invalid(
        "nonminimal-length",
        b"\x30\x83\x00" + len(outer_body).to_bytes(2, "big") + outer_body,
        expected={"a": "accepted", "b": "rejected"},
    )
    add_invalid("overflow-length", b"\x30\x88" + b"\xff" * 8)
    add_invalid("empty", b"")
    add_invalid("exact-1mib", rsa_v3 + bytes(1048576 - len(rsa_v3)))
    add_invalid("over-1mib", bytes(1048577), expected=resource)

    # Preserve the task-1 probe name as an alias seed without using it as a
    # second fixture identity.
    (seeds / "rsa-self-signed.der").write_bytes(base64.b64decode(X509_PROBE_DER_BASE64))
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
    parser.add_argument("--pair", choices=("png", "bmp", "x509"), required=True)
    parser.add_argument("--output", type=Path, required=True)
    arguments = parser.parse_args()
    if arguments.pair == "png":
        generate(arguments.output)
    elif arguments.pair == "bmp":
        generate_bmp(arguments.output)
    else:
        generate_x509(arguments.output)


if __name__ == "__main__":
    main()
