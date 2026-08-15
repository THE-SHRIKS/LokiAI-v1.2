# LokiAI Desktop UI baseline

This directory contains the accepted LokiAI Main v1.2 React/Vite visual and
interaction baseline. It is not yet the production Electron desktop system and
does not prove working authentication, hardware access, monitoring, incidents,
agent delivery, builds, deployments, or updates.

The production plan and rules are indexed in
[`../Docs/README.md`](../Docs/README.md). Do not add privileged Node, filesystem,
shell, serial, credential, or device behavior to this renderer.

## Run Locally

These prototype commands remain only for visual inspection until `FND-002`
establishes the pinned workspace and supported Node/pnpm toolchain.

**Prerequisite:** a compatible local Node.js toolchain


1. Install dependencies:
   `npm install`
2. Run the renderer preview:
   `npm run dev`

No API key or production credential belongs in the renderer or a Vite environment
file. Browser preview is not release end-to-end verification.
