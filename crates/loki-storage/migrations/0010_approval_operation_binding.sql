-- OperationBinding v1. Existing rows were created under an incompatible
-- canonicalization and are deliberately marked version 0 so grant and
-- consumption paths fail closed. Only newly-created rows may use version 1.
ALTER TABLE local_approvals
    ADD COLUMN binding_version INTEGER NOT NULL DEFAULT 0;
ALTER TABLE local_approvals
    ADD COLUMN grant_generation INTEGER NOT NULL DEFAULT 0;
