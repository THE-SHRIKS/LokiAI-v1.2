# Engineering Method Research Basis

Status: **Informative**  
Updated: **2026-08-15**

## 1. Conclusion

“Graph engineering” is not one universally standardized software-development
method. The practical industry pattern is a combination of:

- specification-driven development;
- explicit dependency DAGs;
- orchestrator/worker delegation;
- isolated worker context and write ownership;
- typed artifacts and handoffs;
- independent review and automated gates;
- human approval at high-risk boundaries;
- production evidence flowing back into specifications.

LokiAI adopts that combination without adding an orchestration framework before
it is needed. The task graph is a control artifact; the product runtime's durable
job engine is a separate architectural concern.

## 2. Applied findings

### Specifications must drive tasks

GitHub Spec Kit defines a spec-driven sequence of Spec → Plan → Tasks → Implement
and treats specifications as the source that drives implementation. LokiAI adopts
stable requirement IDs, executable acceptance scenarios, and plan/task artifacts,
but uses its own safety and hardware-aware templates.

Source:

- <https://github.github.com/spec-kit/>
- <https://github.com/github/spec-kit/blob/main/spec-driven.md>

### Multi-agent work should be used only for divisible work

OpenAI's current model guidance describes multi-agent execution as useful for
complex work that divides cleanly into independent streams and notes that it can
reduce wall-clock time. The same guidance emphasizes lean prompts, domain context,
hard constraints, approval boundaries, and success criteria. LokiAI therefore
uses bounded task packets and disjoint file ownership rather than giving every
worker the complete project conversation.

Source:

- <https://developers.openai.com/api/docs/guides/latest-model>

### Subagents work best as isolated specialists

Claude Code documents project subagents as separate contexts with focused prompts,
tool restrictions, permissions, and optional worktree isolation. It recommends
subagents for self-contained work and context-heavy side tasks, while the main
conversation remains better for closely shared context. LokiAI defines planner,
implementer, reviewer, security, and integrator roles and uses isolation only after
Git is established.

Source:

- <https://code.claude.com/docs/en/sub-agents>

### Orchestrator/worker is a graph pattern, not uncontrolled swarm behavior

LangGraph's workflow material describes an orchestrator that decomposes work,
delegates subtasks, and synthesizes outputs, particularly when subtasks cannot all
be predefined. LokiAI makes that process stricter by requiring ready-frontier
calculation, ownership checks, task packets, evidence handoffs, and integration
gates.

Source:

- <https://docs.langchain.com/oss/python/langgraph/workflows-agents>

### Desktop security must follow Electron's trust boundaries

Electron's security guidance treats Electron as more powerful than a browser and
requires controls such as current framework versions, no Node integration for
untrusted content, context isolation, sandboxing, restrictive CSP, navigation and
window controls, sender validation, and narrow API exposure. These are encoded as
release-blocking requirements rather than optional review notes.

Source:

- <https://www.electronjs.org/docs/latest/tutorial/security>
- <https://www.electronjs.org/docs/latest/tutorial/context-isolation>
- <https://www.electronjs.org/docs/latest/tutorial/ipc>

### MCP is a protocol boundary, not an authorization system

The MCP architecture separates host, client, and server participants and uses
JSON-RPC-based data and transport layers. LokiAI keeps its policy, approval,
device lease, durable job, and audit authority in the runtime/control plane; the
MCP server adapts those capabilities for coding clients.

Source:

- <https://modelcontextprotocol.io/docs/learn/architecture>

### The 2026 stack should use supported stable lines, not prototype pins

The review on 2026-08-15 found Electron 43.4.0 as the latest stable release while
44 remained beta; React documents 19.2 as current; Node 24.19.0 is LTS; Vite 8.2
is the regularly patched line; TypeScript 6.0 is the current transition release;
and Rust 1.97.1 is the latest announced stable patch. LokiAI records these as the
review baseline but resolves exact compatible pins at `FND-002`, because current
versions continue to move.

The existing renderer's Vite 6.2/TypeScript 5.8/React 19.0 pins therefore require
a controlled migration. Electron remains suitable only with the documented
renderer/main/runtime trust separation and a supported-line upgrade cadence.

Sources:

- <https://releases.electronjs.org/?channel=stable>
- <https://releases.electronjs.org/schedule>
- <https://vite.dev/releases>
- <https://react.dev/versions>
- <https://nodejs.org/en/about/previous-releases>
- <https://www.typescriptlang.org/docs/handbook/release-notes/typescript-6-0.html>
- <https://blog.rust-lang.org/releases/>

### TypeScript is the production MCP choice for v1.2

The MCP 2026-07-28 release lists TypeScript, Python, Go, and C# as Tier-1 SDKs and
the Rust SDK as beta. LokiAI therefore keeps its privileged domain/runtime in Rust
but implements the provider-neutral MCP adapter in TypeScript. The MCP server is
distributed with Node 24 LTS rather than borrowing Electron's embedded Node.
The same release retires the initialize/session exchange in favor of
self-describing requests and optional discovery, and moves long-running Tasks to
a formal extension. Current provider documentation does not establish that every
supported coding client has moved to that revision: OpenAI documentation still
describes server instructions during initialization, while Claude Code and Cursor
document stdio support with their own feature sets. LokiAI therefore targets the
new revision, retains one tested previous-version edge adapter where certification
proves it necessary, and uses explicit domain IDs plus Tasks/polling adaptation
instead of hidden transport-session authority.

Source:

- <https://blog.modelcontextprotocol.io/posts/2026-07-28/>
- <https://learn.chatgpt.com/docs/extend/mcp?surface=cli>
- <https://docs.anthropic.com/en/docs/claude-code/mcp>
- <https://docs.cursor.com/context/model-context-protocol>

### Packaging separates stable tooling from experimental integration

Electron recommends Electron Forge for packaging, but Forge's Vite plugin remains
explicitly experimental with no API stability guarantee. LokiAI uses stable Vite
builds directly and Forge core/packager/fuses for the Electron application. A WiX
Burn product bundle can chain and sign the desktop/runtime/MCP/build-executor
components required by LokiAI, including service installation behavior.

Sources:

- <https://www.electronjs.org/docs/latest/tutorial/application-distribution/>
- <https://js.electronforge.io/modules/_electron_forge_plugin_vite.html>
- <https://js.electronforge.io/modules/_electron_forge_plugin_fuses.html>
- <https://docs.firegiant.com/wix/tools/burn/>
- <https://docs.firegiant.com/wix/tools/signing/>

### Firmware builds need an operating-system containment boundary

Microsoft documents AppContainer as a kernel-recognized, default-deny boundary
for filesystem, network, process, and other resources. Its sandbox process API
supports explicit file/network policy, low integrity, headless restrictions, and
Job Object controls. LokiAI uses that boundary for repository/toolchain execution
and refuses an unrestricted fallback.

Sources:

- <https://learn.microsoft.com/windows/win32/secauthz/createprocessinsandbox>
- <https://learn.microsoft.com/windows/win32/secauthz/implementing-an-appcontainer>
- <https://learn.microsoft.com/windows/win32/procthread/job-objects>

### Sensitive local SQLite state needs encryption and OS-bound keys

SQLCipher Community provides transparent full-database AES encryption under a
BSD-style license with attribution requirements. Windows DPAPI protects key data
for a user/machine security context. LokiAI keeps OAuth/agent/signing secrets as
separate protected records and uses the protected database key only for encrypted
local state. Edition/support selection remains OQ-007 because production support,
packaging, attribution, and performance are product decisions.

Sources:

- <https://www.zetetic.net/sqlcipher/community/>
- <https://www.zetetic.net/sqlcipher/>
- <https://learn.microsoft.com/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata>

### Security practices belong in every lifecycle phase

NIST SSDF organizes secure practices across preparing, protecting, producing, and
responding to software. LokiAI distributes security responsibilities through the
PRD, architecture, task packets, independent review, release evidence, update, and
incident feedback rather than adding one scan at the end.

Source:

- <https://csrc.nist.gov/pubs/sp/800/218/final>

## 3. What LokiAI deliberately avoids

- one enormous permanent agent prompt;
- parallel workers editing the same files;
- agent-written success claims without command or real-world evidence;
- a reviewer modifying the implementation it is meant to assess;
- product workflow graphs coupled to any one agent provider;
- premature addition of LangGraph or another graph framework to the product;
- using MCP as a direct raw-hardware escape hatch;
- converting visible UI behavior into assumed backend completion.

## 4. Review cadence

Revisit this method after each major release using actual metrics:

- task cycle and correction time;
- merge-conflict/rework rate;
- graph nodes blocked by missing decisions;
- defects found at node, integration, security, hardware, and release gates;
- worker context size and repeated discovery;
- critical-path duration versus parallel worker utilization.

Change the process based on evidence through an ADR or process change, not because
a new agent framework becomes fashionable.
