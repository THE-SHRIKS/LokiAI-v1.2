"""Records the hackathon demo video: a captioned screen recording of the dashboard
driving the REAL board and the REAL Nemotron call. Needs the gateway running.

    python gateway/gateway.py            # terminal 1
    python tools/record_demo.py out_dir  # terminal 2  -> out_dir/<name>.webm  (upload to YouTube)
"""
import sys
import time
from pathlib import Path

from playwright.sync_api import sync_playwright

URL = "http://127.0.0.1:8765"
OUT = Path(sys.argv[1] if len(sys.argv) > 1 else "demo_video")

CAPTION = """(t) => { let d = document.getElementById('cap');
  if (!d) { d = document.createElement('div'); d.id = 'cap';
    d.style.cssText = 'position:fixed;left:0;right:0;bottom:0;padding:16px 40px;background:rgba(10,12,6,.94);color:#e9e7d8;font:600 25px system-ui;text-align:center;border-top:3px solid #a3b03d;z-index:99';
    document.body.append(d); }
  d.textContent = t; }"""
CARD = """([title, sub]) => { let c = document.getElementById('card'); if (c) c.remove();
  if (!title) return; c = document.createElement('div'); c.id = 'card';
  c.style.cssText = 'position:fixed;inset:0;background:#12140e;color:#e9e7d8;display:flex;flex-direction:column;align-items:center;justify-content:center;gap:18px;font-family:system-ui;text-align:center;padding:40px;z-index:100';
  c.innerHTML = '<div style="font-size:54px;font-weight:700;letter-spacing:.03em"></div><div style="font-size:26px;color:#a3b03d;max-width:1000px"></div>';
  c.children[0].textContent = title; c.children[1].textContent = sub; document.body.append(c); }"""


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    with sync_playwright() as p:
        b = p.chromium.launch(channel="chrome", headless=True)
        ctx = b.new_context(viewport={"width": 1280, "height": 720}, record_video_dir=str(OUT), record_video_size={"width": 1280, "height": 720})
        pg = ctx.new_page()
        cap = lambda t: pg.evaluate(CAPTION, t)
        card = lambda t="", s="": pg.evaluate(CARD, [t, s])
        pg.goto(URL)
        pg.wait_for_selector("text=device connected", timeout=20000)
        # fit the whole incident panel (incl. the Approve button) above the caption bar
        pg.evaluate("document.body.style.zoom = '0.72'; document.body.style.paddingBottom = '110px'")

        card("LokiAI Guard", "An AI agent that diagnoses a real ESP32-S3, can only act with human approval, and proves the fix worked.")
        time.sleep(7)
        card("Built on NVIDIA Nemotron + Nebius Token Factory", "Track: Physical AI   |   Hardware: ESP32-S3, no external sensors")
        time.sleep(5)
        card()

        cap("Live telemetry from a real ESP32-S3: heap, chip temperature, uptime. No sensors, just the silicon.")
        time.sleep(7)
        cap("Starting a deliberate memory leak on the real board. The device never reports it. The numbers just fall.")
        pg.click("#inject")
        pg.wait_for_selector(".state", timeout=60000)
        cap("A rule fired (free heap under 40%). An incident opens with an evidence snapshot.")
        time.sleep(4)
        cap("Nemotron, served by Nebius Token Factory, analyses the evidence. Its output is schema-validated and allowlisted.")
        pg.wait_for_selector("text=awaiting approval", timeout=120000)
        cap("Diagnosis: a memory leak. Proposed action: restart. The model has no device authority.")
        time.sleep(12)
        cap("A human approves. The approval is bound to a digest of this exact evidence and expires.")
        time.sleep(4)
        pg.click("text=Approve restart")
        cap("Restart sent. The gateway does NOT trust 'command sent'. It waits for a real reboot.")
        pg.wait_for_selector("text=resolved", timeout=90000)
        cap("Incident closed only after a real reboot and 6 healthy samples were observed on the device.")
        time.sleep(7)
        pg.evaluate("document.getElementById('audit').scrollIntoView()")
        cap("Every step is in a hash-chained audit trail.")
        time.sleep(7)
        card("Roadmap", "Jetson Orin Nano as the always-on gateway  |  MCP so coding agents can request the same bounded actions  |  full LokiAI runtime for flash + rollback")
        time.sleep(7)
        card("LokiAI Guard", "github.com/THE-SHRIKS/LokiAI-v1.2  (branch: akshat)  -  hackathon/loki-guard")
        time.sleep(4)
        vid = pg.video
        ctx.close()
        print("saved:", vid.path())
        b.close()


if __name__ == "__main__":
    main()
