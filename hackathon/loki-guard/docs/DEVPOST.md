# Devpost submission text (copy/paste)

**Track:** Physical AI

**Tagline:** An AI agent that diagnoses a real ESP32-S3 but can only act with human approval, then proves the fix worked.

## Inspiration
Coding agents can already write firmware, but giving them raw serial or flash access to real hardware is unsafe. LokiAI's core idea is to separate reasoning from authority.

## What it does
LokiAI Guard reads live telemetry from an ESP32-S3 (heap, temperature, uptime, reset cause), opens an incident when free memory collapses, asks NVIDIA Nemotron on Nebius Token Factory for a diagnosis and a proposed action, and waits for a human to approve. Approvals are bound to a digest of the exact evidence and expire. The gateway executes the restart and only closes the incident after observing a real reboot and healthy telemetry. Everything is in a hash-chained audit log.

## How we built it
MicroPython firmware on the ESP32-S3; a dependency-light Python gateway (stdlib + pyserial); a vanilla-JS UI; Nemotron via the Token Factory OpenAI-compatible API with strict schema/allowlist validation of model output; tests for the approval and verification logic.

## Challenges
Making "success" mean observed recovery rather than "command sent"; treating device text as untrusted input to the model; no external sensors, so telemetry comes from the chip itself.

## What's next
Jetson Orin Nano as the always-on gateway, MCP tools for coding agents, and the full LokiAI Rust runtime for real flash/rollback.

## Feedback on Token Factory and the NVIDIA models (from real runs)
- **Model used:** `nvidia/NVIDIA-Nemotron-3-Nano-30B-A3B`, found by listing `/v1/models` (30 models, 5 Nemotron variants). I did not find the Nemotron IDs in the docs pages I searched; the model list endpoint was the reliable way. A documented "NVIDIA models on Token Factory" page would help.
- **Integration effort:** the OpenAI-compatible `/v1/chat/completions` worked with plain `urllib` and a bearer key, no SDK needed.
- **Latency:** roughly 14-21 s per diagnosis for a ~270-token prompt, because the reasoning model spends most of its ~830 completion tokens thinking before the JSON. Fine for incident analysis, too slow for a tight control loop.
- **Token budget gotcha:** with `max_tokens=900` one call returned no JSON because reasoning consumed the budget. Raising to 3000 fixed it. The `reasoning` text comes back in a separate field, which made this easy to debug.
- **Instruction following:** with only a schema in the prompt the model chose `monitor_only` for a leak that was still falling. After adding an explicit decision policy it chose `restart_device` in every later run, citing real numbers from the evidence (e.g. "8.29 MB -> 2.19 MB, 99.6% -> 27.1%"). It returned clean JSON every time, with only a `<think>`-style tolerance needed in the parser.
- **Credits:** the promo coupon had not arrived by submission time; the account's existing balance covered about six completions.
