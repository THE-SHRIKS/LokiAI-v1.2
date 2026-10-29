# LokiAI Guard - ESP32-S3 telemetry firmware (MicroPython).
# No external sensors: everything reported is read from the chip itself.
# Output: one JSON line per second on the USB serial console.
# Input (one command per line): leak on | leak off | reboot
import gc
import json
import select
import sys
import time

import esp32
import machine

try:
    import neopixel
except ImportError:
    neopixel = None

RESET_NAMES = {1: "power_on", 2: "hard", 3: "watchdog", 4: "deep_sleep", 5: "soft"}
LEAK_CHUNK = 384 * 1024  # bytes leaked per second while the demo fault is on
LEAK_FLOOR = 1_500_000  # stop leaking below this much free heap so the chip stays alive

# DevKitC-1 boards put the RGB LED on GPIO48 (v1.0) or GPIO38 (v1.1); drive both.
_leds = []
if neopixel:
    for _pin in (48, 38):
        try:
            _leds.append(neopixel.NeoPixel(machine.Pin(_pin), 1))
        except Exception:
            pass


def led(r, g, b):
    for np in _leds:
        np[0] = (r, g, b)
        np.write()


leaking = False
hoard = []
seq = 0
t0 = time.ticks_ms()
reset_cause = RESET_NAMES.get(machine.reset_cause(), "unknown")
poll = select.poll()
poll.register(sys.stdin, select.POLLIN)
line = ""


def handle(cmd):
    global leaking, hoard
    if cmd == "leak on":
        leaking = True
    elif cmd == "leak off":
        leaking = False
        hoard = []
        gc.collect()
    elif cmd == "reboot":
        led(0, 0, 40)
        time.sleep_ms(200)
        machine.reset()


led(0, 40, 0)
while True:
    while poll.poll(0):
        ch = sys.stdin.read(1)
        if ch in ("\r", "\n"):
            if line:
                handle(line.strip())
                line = ""
        else:
            line += ch
    if leaking and gc.mem_free() > LEAK_FLOOR + LEAK_CHUNK:
        hoard.append(bytearray(LEAK_CHUNK))
    free = gc.mem_free()
    total = free + gc.mem_alloc()
    pct = 100.0 * free / total
    led(*((60, 0, 0) if pct < 40 else (0, 40, 0)))
    seq += 1
    print(
        json.dumps(
            {
                "seq": seq,
                "uptime_s": time.ticks_diff(time.ticks_ms(), t0) // 1000,
                "heap_free": free,
                "heap_total": total,
                "free_pct": round(pct, 1),
                "temp_c": round(esp32.mcu_temperature(), 1),
                "reset_cause": reset_cause,
            }
        )
    )
    time.sleep(1)
