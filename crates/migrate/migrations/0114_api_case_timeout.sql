-- Existing cases retain the runner's 30-second default. Long-running OCR/model
-- cases opt in individually, with the same bounded range as domain validation.
ALTER TABLE ms_api_case ADD COLUMN timeout_ms bigint
    CHECK (timeout_ms BETWEEN 1 AND 900000);
