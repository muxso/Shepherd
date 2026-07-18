-- Scenario cron schedules are unified into test plans: mount scenarios on a
-- plan and schedule the plan instead. The standalone schedule table goes away.

DROP TABLE IF EXISTS ms_api_scenario_schedule;
