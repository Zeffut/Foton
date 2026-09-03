from __future__ import annotations

import struct
import sys
from pathlib import Path


def add_utf8(pool, value):
    encoded = value.encode("utf-8")
    index = len(pool)
    pool.append(bytes([1]) + struct.pack(">H", len(encoded)) + encoded)
    return index


def add_class(pool, name_index):
    index = len(pool)
    pool.append(bytes([7]) + struct.pack(">H", name_index))
    return index


def add_name_type(pool, name_index, descriptor_index):
    index = len(pool)
    pool.append(bytes([12]) + struct.pack(">HH", name_index, descriptor_index))
    return index


def add_interface_method(pool, class_index, name_type_index):
    index = len(pool)
    pool.append(bytes([11]) + struct.pack(">HH", class_index, name_type_index))
    return index


def pool_end(data):
    count = struct.unpack_from(">H", data, 8)[0]
    offset = 10
    pool = [None]
    index = 1
    while index < count:
        tag = data[offset]
        start = offset
        offset += 1
        if tag == 1:
            length = struct.unpack_from(">H", data, offset)[0]
            offset += 2 + length
        else:
            width = {3: 4, 4: 4, 5: 8, 6: 8, 7: 2, 8: 2, 9: 4,
                     10: 4, 11: 4, 12: 4, 15: 3, 16: 2, 17: 4,
                     18: 4, 19: 2, 20: 2}[tag]
            offset += width
        pool.append(data[start:offset])
        if tag in (5, 6):
            pool.append(None)
            index += 2
        else:
            index += 1
    return count, offset, pool


def skip_attributes(data, offset):
    count = struct.unpack_from(">H", data, offset)[0]
    offset += 2
    for _ in range(count):
        length = struct.unpack_from(">I", data, offset + 2)[0]
        offset += 6 + length
    return offset


def method_table_offset(data, cp_offset):
    offset = cp_offset + 6
    interfaces = struct.unpack_from(">H", data, offset)[0]
    offset += 2 + 2 * interfaces
    method_count_offset = None
    for table in range(2):
        if table == 1:
            method_count_offset = offset
        count = struct.unpack_from(">H", data, offset)[0]
        offset += 2
        for _ in range(count):
            offset += 6
            offset = skip_attributes(data, offset)
    return method_count_offset, offset

def has_string_target(data, cp_offset, pool):
    method_count_offset, _ = method_table_offset(data, cp_offset)
    offset = method_count_offset
    count = struct.unpack_from(">H", data, offset)[0]
    offset += 2
    for _ in range(count):
        name_index, descriptor_index = struct.unpack_from(">HH", data, offset + 2)
        def text(index):
            raw = pool[index]
            length = struct.unpack_from(">H", raw, 1)[0]
            return raw[3:3 + length].decode("utf-8")
        if text(name_index) == "getTarget" and text(descriptor_index) == "()Ljava/lang/String;":
            return True
        offset += 6
        offset = skip_attributes(data, offset)
    return False

def transform(data):
    count, cp_offset, pool = pool_end(data)
    this_class = struct.unpack_from(">H", data, cp_offset + 2)[0]
    def class_name(index):
        entry = pool[index]
        name_index = struct.unpack_from(">H", entry, 1)[0]
        raw = pool[name_index]
        length = struct.unpack_from(">H", raw, 1)[0]
        return raw[3:3 + length].decode("utf-8")
    if class_name(this_class) != "org/bukkit/BanEntry":
        return data
    if has_string_target(data, cp_offset, pool):
        return data
    name = add_utf8(pool, "getTarget")
    string_descriptor = add_utf8(pool, "()Ljava/lang/String;")
    code_name = add_utf8(pool, "Code")
    object_descriptor = add_utf8(pool, "()Ljava/lang/Object;")
    ban_name = add_utf8(pool, "org/bukkit/BanEntry")
    ban_class = add_class(pool, ban_name)
    string_name = add_utf8(pool, "java/lang/String")
    string_class = add_class(pool, string_name)
    name_type = add_name_type(pool, name, object_descriptor)
    target_ref = add_interface_method(pool, ban_class, name_type)
    cp_bytes = b"".join(entry for entry in pool[1:] if entry is not None)
    method_count_offset, method_offset = method_table_offset(data, cp_offset)
    insertion = method_offset - cp_offset
    method = struct.pack(">HHHH", 0x0001, name, string_descriptor, 1) + struct.pack(">HI", code_name, 22)
    code = bytes([0x2A, 0xB9, target_ref >> 8, target_ref & 0xFF, 0x01, 0x00,
                  0xC0, string_class >> 8, string_class & 0xFF, 0xB0])
    method += struct.pack(">HHI", 1, 1, len(code)) + code + struct.pack(">HH", 0, 0)
    body = data[cp_offset:]
    prefix = bytearray(body[:insertion])
    count_at = method_count_offset - cp_offset
    struct.pack_into(">H", prefix, count_at, struct.unpack_from(">H", prefix, count_at)[0] + 1)
    return (data[:8] + struct.pack(">H", len(pool)) + cp_bytes +
            bytes(prefix) + method + body[insertion:])


if __name__ == "__main__":
    path = Path(sys.argv[1])
    path.write_bytes(transform(path.read_bytes()))
