# LokiAI Guard

**An AI agent that watches a real ESP32-S3, diagnoses faults with NVIDIA Nemotron on Nebius Token Factory, and can only act after a human approves. The gateway then verifies the fix on the device before closing the incident.**

Built for the Nebius x NVIDIA Global AI Hackathon (track: Physical AI). It is the hackathon-scale slice of [LokiAI](../../Docs/CONTEXT.md) (full design docs in `Docs/` at the repo root), whose core idea is *separating reasoning from authority*: agents investigate and propose; LokiAI authenticates, executes, observes and records; humans keep final say.

## What it does

```
ESP32-S3 (MicroPython, real chip telemetry)
   │ JSON lines over USB serial, 1 Hz
   ▼
gateway.py ── rule: free heap < 40% for 3 samples ──► incident + evidence snapshot
   │                                                      │
   │                          Nemotron on Nebius Token Factory (diagnosis, severity, proposed action)
   │                                                      │
   │                    strict validation: action must be on the allowlist
   ▼                                                      ▼
Web UI ◄── approval bound to a SHA-256 digest of (incident, action, device, evidence), expires in 180 s
   │ human clicks Approve
   ▼
gateway sends `reboot` ──► waits for a real reboot (uptime reset) + 6 healthy samples ──► incident resolved
```

Every step is appended to a hash-chained audit log (`gateway/audit.jsonl`).

**No sensors needed.** All telemetry comes from the chip itself: heap (internal + 8 MB PSRAM), internal temperature, uptime, reset cause. The "fault" is a deliberate memory leak that you start from the UI (`Demo: inject memory leak`). The device never reports that it is leaking; the model has to infer it from the numbers.

## How NVIDIA Nemotron and Nebius Token Factory are used

* `gateway/gateway.py` (`Nemotron` class) calls the Token Factory OpenAI-compatible API (`/v1/chat/completions`) at incident time. If `NEBIUS_MODEL` is unset it lists `/v1/models` and picks a Nemotron model from the live catalog.
* The model returns a JSON diagnosis (hypothesis, severity, confidence, reasoning, risks) and one of two allowlisted actions: `restart_device` or `monitor_only`. Anything else is rejected, so a prompt-injected or hallucinated action can never reach the device.
* Device text is passed as untrusted data inside `<evidence>` tags, and the model never sends device commands. Only the gateway does, after approval.
* If no API key is set the UI says so honestly ("AI analysis unavailable") and a human can still decide.

## Run it

Requirements: Python 3.10+, an ESP32-S3 on USB, a Nebius Token Factory API key.

```bash
pip install -r requirements.txt esptool mpremote

# 1. Flash MicroPython (ESP32_GENERIC_S3 or SPIRAM_OCT build from micropython.org), then:
python -m mpremote fs cp firmware/main.py :main.py
python -m mpremote reset

# 2. Configure the analyst
cp .env.example .env              # then put your NEBIUS_API_KEY in .env (gateway/.env also works)

# 3. Run the gateway and open http://127.0.0.1:8765
python gateway/gateway.py
```

The board is auto-detected by its Espressif USB id (`303A`); set `LOKI_PORT` to override. Try it: click **Demo: inject memory leak**, wait ~15 s for the incident, read the diagnosis, click **Approve restart**, and watch the verification complete.

## Re-record the demo video

```bash
python tools/record_demo.py demo_video   # needs: pip install playwright && playwright install ffmpeg, Chrome installed, gateway running
```

## Tests

```bash
python tests/test_guard.py
```

Covers: the rule threshold, digest mismatch/replay rejection, one reboot per approval, no resolution without an observed reboot, allowlist enforcement, `monitor_only` creating nothing to approve, and the no-API-key path.

## Safety design (inherited from LokiAI)

* The model proposes; it holds no device authority.
* Approvals are single-purpose, time-limited and bound to the exact evidence; a changed input invalidates them.
* "Done" means *observed* healthy, not "command sent".
* Append-only, hash-chained audit trail.

## Roadmap

NVIDIA Jetson Orin Nano as the always-on gateway host; MCP tools so coding agents (Claude Code, Codex, Cursor) can request the same bounded actions; real firmware flash and rollback through the full LokiAI runtime in this repo; NeMo Guardrails in front of proposed actions.

## License

MIT, see `LICENSE`.
