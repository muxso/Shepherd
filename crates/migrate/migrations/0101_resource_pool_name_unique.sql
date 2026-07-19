-- Pool-runner registration resolves pools by name, so live names must be unique.
CREATE UNIQUE INDEX IF NOT EXISTS ux_resource_pool_live_name
    ON ms_resource_pool (name) WHERE NOT deleted;
