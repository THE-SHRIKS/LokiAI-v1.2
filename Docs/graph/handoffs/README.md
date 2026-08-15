# Worker handoff registry

This directory contains structured outcomes from dispatched graph nodes. Use
`../templates/WORKER_HANDOFF.md` and name each record
`<NODE-ID>-<ATTEMPT>.md`.

A handoff records observed results; it does not change graph status. Only the
orchestrator may accept the result and update `../TASK_GRAPH.md`. Do not store
secrets, raw customer payloads, production identifiers, or large generated logs
here. Reference protected evidence by location and cryptographic digest.

No worker handoffs have been accepted yet.
