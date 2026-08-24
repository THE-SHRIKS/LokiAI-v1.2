# DAT-002 Independent Security Review 002

Result: `FAILED_VERIFICATION`  
Reviewed revision: `7f486de21cfe5713be03ae3e1cbfb6655492c427`  
Date: `2026-08-24`

Scoped storage/runtime tests passed, but the independent review found eight
unresolved high-severity failures not discriminated by those tests:

1. Recovery-generation crashes can split database/key generations and cleanup
   can delete the last complete recovery pair.
2. Managed backups do not carry an independently retained matching protected key.
3. Legacy plaintext IPC tokens survive when protected-token loading fails.
4. First-use recovery ignores real SQLite WAL/SHM/journal sidecars.
5. Interrupted recovery replaces files without the non-cooperative database write fence.
6. Corrupted DPAPI ciphertext is misclassified as identity-inaccessible.
7. SQLCipher compatibility-4 parameters are not verified against one exact locked profile.
8. Backup files can exist before their protected ACL is established atomically.

Required correction includes exhaustive fault injection across generation/live
promotion and cleanup, independently restorable backup DB/key generations,
unconditional legacy cleanup, sidecar-aware first-use recovery, fenced recovery,
versioned corruption classification, exact SQLCipher-profile assertions, and
atomic backup ACL creation with adversarial child-process tests.

A versioned envelope length plus SHA-256 checksum may classify corruption; it is
not an authentication boundary. Checksum failure is malformed, an intact new
envelope whose DPAPI unwrap fails is identity-inaccessible, and unverifiable raw
legacy unwrap failure is a distinct ambiguous-legacy state.

The mandatory real second-identity/VM matrix is still absent and independently
blocks DAT-002 acceptance. No mock or same-identity child test may satisfy it.
