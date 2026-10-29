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

## Feedback on Token Factory and NVIDIA models (fill in after running)
- Model(s) used and latency:
- What worked well:
- What was hard / missing:
