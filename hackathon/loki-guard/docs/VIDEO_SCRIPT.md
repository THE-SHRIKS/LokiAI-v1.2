# Demo video script (target 2:30, under the 3:00 limit, no music)

Setup before recording: board on USB, `python gateway/gateway.py` running, browser on http://127.0.0.1:8765, key set. Screen-record the browser and film the board's LED if possible (green = healthy, red = low memory, blue = rebooting).

| Time | Show | Say |
|---|---|---|
| 0:00 | Title card / board on desk | "Coding agents can write firmware, but nobody lets them near real hardware, and for good reason." |
| 0:20 | Gateway UI, live telemetry | "LokiAI Guard sits between the agent and an ESP32-S3. These are real readings from the chip: heap, temperature, uptime. No sensors, just the silicon." |
| 0:45 | Click "inject memory leak"; chart falls | "I'll start a memory leak on the real board. The device never says it is leaking; the numbers just fall." |
| 1:05 | Incident opens | "A rule opens an incident and captures evidence. Now Nemotron, running on Nebius Token Factory, gets that evidence." |
| 1:20 | Diagnosis card | "It diagnoses a leak from the heap slope and proposes one action, restart. The proposal must be on an allowlist; the model never touches the device." |
| 1:45 | Click Approve | "A human approves. The approval is bound to this exact evidence and expires." |
| 2:00 | Board LED blue, chart recovers, "resolved" | "The gateway doesn't trust 'command sent'. It waits for a real reboot and six healthy samples, then closes the incident." |
| 2:20 | Audit trail | "Every step is in a hash-chained audit log." |
| 2:30 | Roadmap slide | "Next: a Jetson Orin Nano as the always-on gateway, and MCP so any coding agent can request the same bounded actions." |
