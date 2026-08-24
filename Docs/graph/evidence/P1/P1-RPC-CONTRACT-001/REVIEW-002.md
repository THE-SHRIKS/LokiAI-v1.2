# P1-RPC-CONTRACT-001 Independent Review 002

Result: `FAILED_VERIFICATION`  
Reviewed code: `74c691e5af7a3332535dc612edbe0aae8e4a194a`  
Reviewed handoff: `050e501`  
Date: `2026-08-24`

The first five high findings were materially corrected and the independent
worker gates reproduced: Rust 89/89, TypeScript 312/312, typecheck, formatting
and warnings-denied Clippy. Three high findings remain:

1. Credential expiry/revocation is resolved only during Hello. An established
   privileged session retains its cloned principal indefinitely, including new
   calls and subscriptions after revocation or expiry.
2. Unary methods do not enforce workspace/entity scope before producer dispatch,
   and request/result matching checks only variants rather than requested device,
   job, incident, run, intent or approval identity.
3. TypeScript result variants are strict, but Rust business result variants
   accept unknown top-level fields, so exact output parity is false.

Required correction: dynamic session revalidation without retaining plaintext
credentials; complete per-method workspace/entity scope and result-identity
coupling; strict Rust result decoding with shared extra-field negatives; real
post-handshake revoke/expiry and cross-scope pipe tests. The final evidence must
also exercise all 23 unary variants, all 10 streams, and business-specific
cancellation, deadlines and slow-consumer backpressure.

No review edits were made. Integration remains blocked pending correction and a
new independent review with no critical/high finding.
