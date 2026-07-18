-- Per-phase HTTP timings on case results (dnsMs / ttfbMs / downloadMs),
-- rendered as a waterfall on latency hover in reports.

ALTER TABLE ms_api_case_result ADD COLUMN IF NOT EXISTS timings jsonb;
