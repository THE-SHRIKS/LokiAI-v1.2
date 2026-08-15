#!/usr/bin/env node
// LokiAI Main v1.2 — FND-002.
//
// The root Cargo workspace is intentionally empty until RUN-001/CON-001/etc.
// add real crates. `cargo build --workspace` (and clippy/test) genuinely error
// with "the workspace has no members" on zero members — that's correct Cargo
// behavior, not a bug in this scaffold. This guard makes that state an honest,
// visible skip instead of either a false CI failure or a silently-faked pass.
// The moment a real crate is added to Cargo.toml's `members`, this guard gets
// out of the way and the real `cargo` command runs and is enforced normally.
import { execFileSync } from 'node:child_process';

const [, , ...cargoArgs] = process.argv;

if (cargoArgs.length === 0) {
  console.error('cargo-verify-guard: no cargo subcommand/args provided');
  process.exit(1);
}

let memberCount;
try {
  const raw = execFileSync('cargo', ['metadata', '--no-deps', '--format-version', '1'], {
    encoding: 'utf8',
  });
  memberCount = JSON.parse(raw).workspace_members.length;
} catch (err) {
  console.error(
    'cargo-verify-guard: `cargo metadata` failed — treating as a real failure, not a skip.',
  );
  console.error(err.message);
  process.exit(1);
}

if (memberCount === 0) {
  console.log(
    `cargo-verify-guard: workspace has 0 members — skipping "cargo ${cargoArgs.join(' ')}" ` +
      '(expected until a node adds the first crate; not a passing result).',
  );
  process.exit(0);
}

try {
  execFileSync('cargo', cargoArgs, { stdio: 'inherit' });
} catch (err) {
  process.exit(typeof err.status === 'number' ? err.status : 1);
}
