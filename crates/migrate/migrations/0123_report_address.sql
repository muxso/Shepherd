CREATE TABLE IF NOT EXISTS ms_report_address (
    id SMALLINT PRIMARY KEY CHECK (id = 1),
    base_url TEXT NOT NULL
);
