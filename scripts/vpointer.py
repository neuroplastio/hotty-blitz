#!/usr/bin/env python3
"""A virtual pointer for the private headless display.

    vpointer.py <width> <height> move X Y [click] [move X Y click ...]
    vpointer.py <width> <height> move X Y down move X Y ... up   (a drag)

Speaks the Wayland wire protocol directly (no dependencies) to create a
zwlr_virtual_pointer_v1 and move and click it in logical pixels of an output
of the given size. `click` presses and releases the left button; `down` and
`up` are its halves, for a drag.
"""
import os
import socket
import struct
import sys
import time

BTN_LEFT = 0x110


def pad(b):
    return b + b"\0" * (-len(b) % 4)


def wl_string(s):
    b = s.encode() + b"\0"
    return struct.pack("<I", len(b)) + pad(b)


class Conn:
    def __init__(self):
        path = os.path.join(os.environ["XDG_RUNTIME_DIR"], os.environ["WAYLAND_DISPLAY"])
        self.s = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.s.connect(path)
        self.next_id = 2
        self.buf = b""

    def new_id(self):
        i = self.next_id
        self.next_id += 1
        return i

    def send(self, obj, opcode, args=b""):
        size = 8 + len(args)
        self.s.sendall(struct.pack("<IHH", obj, opcode, size) + args)

    def events(self, timeout=0.3):
        self.s.settimeout(timeout)
        try:
            while True:
                chunk = self.s.recv(65536)
                if not chunk:
                    break
                self.buf += chunk
        except socket.timeout:
            pass
        out = []
        while len(self.buf) >= 8:
            obj, op, size = struct.unpack("<IHH", self.buf[:8])
            if len(self.buf) < size:
                break
            out.append((obj, op, self.buf[8:size]))
            self.buf = self.buf[size:]
        return out


def main():
    w, h = int(sys.argv[1]), int(sys.argv[2])
    steps = sys.argv[3:]
    c = Conn()
    registry = c.new_id()
    c.send(1, 1, struct.pack("<I", registry))  # wl_display.get_registry
    callback = c.new_id()
    c.send(1, 0, struct.pack("<I", callback))  # wl_display.sync
    manager_name = None
    version = 1
    for obj, op, body in c.events(0.5):
        if obj == registry and op == 0:  # wl_registry.global(name, interface, version)
            name = struct.unpack("<I", body[:4])[0]
            ln = struct.unpack("<I", body[4:8])[0]
            iface = body[8 : 8 + ln - 1].decode()
            ver = struct.unpack("<I", body[8 + ((ln + 3) & ~3) : 12 + ((ln + 3) & ~3)])[0]
            if iface == "zwlr_virtual_pointer_manager_v1":
                manager_name, version = name, min(ver, 2)
    if manager_name is None:
        sys.exit("no zwlr_virtual_pointer_manager_v1 on this display")
    manager = c.new_id()
    c.send(
        registry,
        0,
        struct.pack("<I", manager_name) + wl_string("zwlr_virtual_pointer_manager_v1") + struct.pack("<II", version, manager),
    )
    pointer = c.new_id()
    c.send(manager, 0, struct.pack("<II", 0, pointer))  # create_virtual_pointer(seat=null, id)
    time.sleep(0.2)

    def ms():
        return int(time.monotonic() * 1000) & 0xFFFFFFFF

    i = 0
    while i < len(steps):
        if steps[i] == "move":
            x, y = int(steps[i + 1]), int(steps[i + 2])
            c.send(pointer, 1, struct.pack("<IIIII", ms(), x, y, w, h))  # motion_absolute
            c.send(pointer, 4)  # frame
            i += 3
        elif steps[i] == "click":
            c.send(pointer, 2, struct.pack("<III", ms(), BTN_LEFT, 1))
            c.send(pointer, 4)
            time.sleep(0.05)
            c.send(pointer, 2, struct.pack("<III", ms(), BTN_LEFT, 0))
            c.send(pointer, 4)
            i += 1
        elif steps[i] in ("down", "up"):  # half a click, for drags
            c.send(pointer, 2, struct.pack("<III", ms(), BTN_LEFT, int(steps[i] == "down")))
            c.send(pointer, 4)
            i += 1
        elif steps[i] == "sleep":
            time.sleep(float(steps[i + 1]))
            i += 2
        else:
            sys.exit(f"unknown step {steps[i]}")
        time.sleep(0.05)
    c.events(0.2)


if __name__ == "__main__":
    main()
