#!/usr/bin/env python3
"""
Convert PrismarineJS loginPacket.json (1.21.1) into per-registry binary blobs
suitable for embedding via `include_bytes!` and shipping in
`S2CRegistryData.data` for each registry packet.

The output format for each .bin matches the protodef-defined wire format of
`S2CRegistryData.data` AFTER the leading registry_id field — i.e., the
encoder of `S2CRegistryData` writes:
    String(registry_id) + <contents of .bin>

The .bin contents per registry are:
    VarInt(entry_count)
    for each entry:
        String(key)          # VarInt length-prefixed UTF-8
        bool(value_present)  # 1 (always present in our generated data)
        anonymous_nbt_bytes  # one Minecraft tag (TAG_Compound = 0x0A) with
                             # NO outer name (just type byte + payload + END)

`anonymous_nbt` here is the format introduced in 1.20.2 where the root NBT
tag of network packets no longer carries an outer name. Earlier protocols
used `TAG_Compound + u16(name_len=0) + payload`; from 1.20.2 onward it's
just `TAG_Compound + payload`.

Input format: prismarine-nbt JSON (each node is `{type, value}` with
`type ∈ {byte,short,int,long,float,double,string,list,compound,
intArray,longArray,byteArray}` etc.).

Usage:
    python3 build_tools/build_registry_nbt.py \
        tmp/minecraft-data/data/pc/1.21.1/loginPacket.json \
        crates/statik_proto/src/v1_21_1/data
"""

from __future__ import annotations

import json
import struct
import sys
from pathlib import Path

# Minecraft NBT tag type ids.
TAG_END        = 0x00
TAG_BYTE       = 0x01
TAG_SHORT      = 0x02
TAG_INT        = 0x03
TAG_LONG       = 0x04
TAG_FLOAT      = 0x05
TAG_DOUBLE     = 0x06
TAG_BYTE_ARRAY = 0x07
TAG_STRING     = 0x08
TAG_LIST       = 0x09
TAG_COMPOUND   = 0x0a
TAG_INT_ARRAY  = 0x0b
TAG_LONG_ARRAY = 0x0c

NAME_TO_TAG = {
    "end":       TAG_END,
    "byte":      TAG_BYTE,
    "short":     TAG_SHORT,
    "int":       TAG_INT,
    "long":      TAG_LONG,
    "float":     TAG_FLOAT,
    "double":    TAG_DOUBLE,
    "byteArray": TAG_BYTE_ARRAY,
    "string":    TAG_STRING,
    "list":      TAG_LIST,
    "compound":  TAG_COMPOUND,
    "intArray":  TAG_INT_ARRAY,
    "longArray": TAG_LONG_ARRAY,
}


def encode_varint(n: int) -> bytes:
    """Standard 32-bit zig-zag-free VarInt (Mojang signed-unsigned variant)."""
    if n < 0:
        n &= 0xFFFFFFFF
    out = bytearray()
    while True:
        b = n & 0x7F
        n >>= 7
        if n != 0:
            out.append(b | 0x80)
        else:
            out.append(b)
            break
    return bytes(out)


def encode_string(s: str) -> bytes:
    raw = s.encode("utf-8")
    return encode_varint(len(raw)) + raw


def encode_mc_short_string(s: str) -> bytes:
    """NBT-style string: u16 BE length + UTF-8."""
    raw = s.encode("utf-8")
    return struct.pack(">H", len(raw)) + raw


def encode_payload(tag: int, val) -> bytes:
    """Encode the payload of a tag (without type byte / name)."""
    if tag == TAG_BYTE:
        # python: int (may be signed); pack signed i8
        return struct.pack(">b", val & 0xFF if val >= 0 else val)
    if tag == TAG_SHORT:
        return struct.pack(">h", val)
    if tag == TAG_INT:
        return struct.pack(">i", val)
    if tag == TAG_LONG:
        # prismarine-nbt encodes long as [hi, lo] 32-bit pair OR a string.
        if isinstance(val, list):
            hi, lo = val
            return struct.pack(">i", hi) + struct.pack(">I", lo & 0xFFFFFFFF)
        return struct.pack(">q", val)
    if tag == TAG_FLOAT:
        return struct.pack(">f", val)
    if tag == TAG_DOUBLE:
        return struct.pack(">d", val)
    if tag == TAG_BYTE_ARRAY:
        return struct.pack(">i", len(val)) + bytes(b & 0xFF for b in val)
    if tag == TAG_STRING:
        return encode_mc_short_string(val)
    if tag == TAG_LIST:
        # prismarine-nbt list = {type: 'list', value: {type: '<innerTag>', value: [...]}}
        # but here `val` is already the inner dict.
        inner_type_name = val["type"]
        inner_tag = NAME_TO_TAG[inner_type_name]
        items = val["value"]
        if inner_tag == TAG_END:
            # empty list: payload is { tag_byte=0, length=0 }
            return struct.pack(">b", 0) + struct.pack(">i", 0)
        out = bytearray()
        out.append(inner_tag)
        out += struct.pack(">i", len(items))
        for it in items:
            # `it` is the raw value for the inner tag (no {type,value} wrapper).
            if inner_tag == TAG_COMPOUND:
                # `it` IS the inner mapping of named tags.
                out += encode_compound_body(it)
            elif inner_tag == TAG_LIST:
                # nested list: `it` is a list-shape dict
                out += encode_payload(TAG_LIST, it)
            else:
                out += encode_payload(inner_tag, it)
        return bytes(out)
    if tag == TAG_COMPOUND:
        # val is { name: {type, value}, ... }
        return encode_compound_body(val)
    if tag == TAG_INT_ARRAY:
        return struct.pack(">i", len(val)) + b"".join(struct.pack(">i", n) for n in val)
    if tag == TAG_LONG_ARRAY:
        # val items may be [hi, lo] pairs or ints
        body = []
        for n in val:
            if isinstance(n, list):
                hi, lo = n
                body.append(struct.pack(">i", hi) + struct.pack(">I", lo & 0xFFFFFFFF))
            else:
                body.append(struct.pack(">q", n))
        return struct.pack(">i", len(val)) + b"".join(body)
    raise ValueError(f"unknown tag {tag}")


def encode_compound_body(named_map: dict) -> bytes:
    """Encode the body of a TAG_Compound (named entries + final TAG_End)."""
    out = bytearray()
    for name, child in named_map.items():
        ctype_name = child["type"]
        cval = child["value"]
        ctag = NAME_TO_TAG[ctype_name]
        out.append(ctag)
        out += encode_mc_short_string(name)
        out += encode_payload(ctag, cval)
    out.append(TAG_END)
    return bytes(out)


def encode_anonymous_nbt(node: dict) -> bytes:
    """Encode a top-level NBT tag in the 'anonymous' (1.20.2+) form: just
    `tag_type_byte || payload`, no outer name.
    """
    tag_name = node["type"]
    tag = NAME_TO_TAG[tag_name]
    out = bytearray()
    out.append(tag)
    out += encode_payload(tag, node["value"])
    return bytes(out)


def sanitize_id(registry_id: str) -> str:
    return registry_id.replace("minecraft:", "").replace("/", "_")


def build_registry_blob(entries: list) -> bytes:
    """Build the `S2CRegistryData.data` content (after the leading
    registry_id String): VarInt(count) + N × (String(key) + bool + nbt).
    """
    out = bytearray()
    out += encode_varint(len(entries))
    for entry in entries:
        key = entry["key"]
        val = entry["value"]   # prismarine-nbt {type, value}
        out += encode_string(key)
        out.append(0x01)        # "value present" bool
        out += encode_anonymous_nbt(val)
    return bytes(out)


def main(argv):
    if len(argv) != 3:
        print(__doc__, file=sys.stderr)
        return 1
    src = Path(argv[1])
    dst = Path(argv[2])
    dst.mkdir(parents=True, exist_ok=True)

    data = json.loads(src.read_text())
    dimension_codec = data["dimensionCodec"]

    # statik's Configuration burst sends these registries (a subset of the
    # Notchian set is enough for the limbo path: dimension_type, biome,
    # chat_type, damage_type, trim_pattern, trim_material). We still
    # generate the full set in dimensionCodec so we can extend later.
    written = []
    for registry_id, body in dimension_codec.items():
        entries = body.get("entries", [])
        blob = build_registry_blob(entries)
        out_name = f"registry_{sanitize_id(registry_id)}.bin"
        out_path = dst / out_name
        out_path.write_bytes(blob)
        written.append((registry_id, out_name, len(entries), len(blob)))

    for rid, name, n, sz in written:
        print(f"{rid:<35s} -> {name:<50s} ({n:3d} entries, {sz:6d} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
