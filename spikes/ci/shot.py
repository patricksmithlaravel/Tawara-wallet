#!/usr/bin/env python3
"""Screenshot helper for the Android CI checklist (stdlib only).

  shot.py stats RAW            -> one line: w h fmt sha256 unique_colors nonbg_permille
  shot.py thumb RAW OUT.png N  -> writes a 1/N nearest-neighbour PNG thumbnail
  shot.py diff RAW_A RAW_B     -> permille of sampled pixels that differ

RAW is the output of `adb exec-out screencap` (no -p): a header of
width, height, pixel-format (u32 LE each) followed, on recent Android
releases, by a u32 dataspace, then width*height*4 bytes. The header length
is derived from the file size, so 12- and 16-byte headers both work.
"""
import hashlib
import struct
import sys
import zlib


def load(path):
    data = open(path, "rb").read()
    w, h, fmt = struct.unpack_from("<III", data, 0)
    bpp = 4  # RGBA_8888 (1), RGBX_8888 (2), BGRA_8888 (5) are all 4 bytes
    hdr = len(data) - w * h * bpp
    if hdr not in (12, 16):
        raise SystemExit(f"unexpected raw size {len(data)} for {w}x{h}")
    return w, h, fmt, memoryview(data)[hdr:]


def px(buf, w, x, y):
    o = (y * w + x) * 4
    return bytes(buf[o:o + 3])  # ignore alpha


def stats(path):
    w, h, fmt, buf = load(path)
    step = max(1, min(w, h) // 120)
    seen, total, nonbg = set(), 0, 0
    bg = px(buf, w, w // 2, h - 2)  # sample near the bottom edge as "background"
    for y in range(0, h, step):
        for x in range(0, w, step):
            p = px(buf, w, x, y)
            seen.add(p)
            total += 1
            if p != bg:
                nonbg += 1
    digest = hashlib.sha256(buf).hexdigest()[:16]
    print(w, h, fmt, digest, len(seen), nonbg * 1000 // max(total, 1))


def diff(a, b):
    wa, ha, _, ba = load(a)
    wb, hb, _, bb = load(b)
    if (wa, ha) != (wb, hb):
        print(1000)
        return
    step = max(1, min(wa, ha) // 120)
    total = changed = 0
    for y in range(0, ha, step):
        for x in range(0, wa, step):
            total += 1
            if px(ba, wa, x, y) != px(bb, wa, x, y):
                changed += 1
    print(changed * 1000 // max(total, 1))


def thumb(path, out, n):
    w, h, fmt, buf = load(path)
    bgr = fmt == 5  # HAL_PIXEL_FORMAT_BGRA_8888
    tw, th = w // n, h // n
    rows = bytearray()
    for ty in range(th):
        rows.append(0)  # PNG filter type 0
        y = ty * n
        for tx in range(tw):
            p = px(buf, w, tx * n, y)
            rows += p[::-1] if bgr else p
    def chunk(tag, body):
        c = struct.pack(">I", len(body)) + tag + body
        return c + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF)
    png = b"\x89PNG\r\n\x1a\n"
    png += chunk(b"IHDR", struct.pack(">IIBBBBB", tw, th, 8, 2, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(bytes(rows), 9))
    png += chunk(b"IEND", b"")
    open(out, "wb").write(png)


if __name__ == "__main__":
    cmd = sys.argv[1]
    if cmd == "stats":
        stats(sys.argv[2])
    elif cmd == "diff":
        diff(sys.argv[2], sys.argv[3])
    elif cmd == "thumb":
        thumb(sys.argv[2], sys.argv[3], int(sys.argv[4]))
    else:
        raise SystemExit(__doc__)
