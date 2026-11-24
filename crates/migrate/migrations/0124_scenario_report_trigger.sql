ALTER TABLE ms_api_batch_report ADD COLUMN scenario_triggered_by TEXT;
CREATE INDEX ix_api_batch_report_scenario_trigger
    ON ms_api_batch_report (finished_at) WHERE scenario_triggered_by IS NOT NULL;
CREATE INDEX ix_api_scenario_execution_report
    ON ms_api_scenario_execution (report_id) WHERE report_id IS NOT NULL;
