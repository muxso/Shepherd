# Environment capabilities plugin

Tracks implementation in [#150](https://github.com/muxso/Shepherd/issues/150).

The host discovers typed provider forms and stores no provider-specific columns in business tables. Profiles, environment bindings, migration history, and encrypted credentials live in a separately named PostgreSQL database owned by this plugin. Environment/project references are validated by the host before management or execution.

## Enable

The `environment-plugins` server feature is included in default builds. A build with `--no-default-features` still exposes authenticated capability discovery and reports the plugin as not installed.

1. Create an independent database with a different name from the Shepherd database and a dedicated account.
2. Add `environment` to the comma-separated `SHEPHERD_PLUGINS` value.
3. Set `SHEPHERD_ENVIRONMENT_PLUGINS_DATABASE_URL` to the plugin database.
4. Set `SHEPHERD_ENVIRONMENT_PLUGIN_KEYS` to a private JSON map such as `{"v1":"<base64-encoded 32-byte key>"}`. Set `SHEPHERD_ENVIRONMENT_PLUGIN_ACTIVE_KEY=v1`. Generate key material with `openssl rand -base64 32`; keep it outside the repository.
5. Restart and open a saved project's Environment page. Only registered provider capabilities appear as configurable forms; other sections report that no provider is available.

The database name guard runs before migrations and rejects host database names even through alternate connection addresses. An enabled plugin with missing storage or encryption configuration fails startup rather than using host storage. A disabled plugin does not open or migrate its database.

## Current behavior

The initial host integration supports profile CRUD, optimistic revisions, environment bindings, encrypted credential references, and real PostgreSQL/MySQL connection tests, parameterized statements, bounded row results, variable extraction, and downloadable reports through the probe engines. Shared runner integration remains a separate TODO.

Profiles are project scoped and may be shared across environments. New credentials created in the UI are restricted to the current environment. To reuse a profile elsewhere, provide a credential valid in that environment. Editing a shared profile affects its existing bindings; deleting a bound profile is rejected.

Management requires environment read/update permission and project membership. Viewers can inspect configuration but cannot modify or execute it. Execution additionally requires an applicable API definition/scenario/test-plan/runner execution permission and an enabled environment. Administrators retain access across projects. Caller-provided actor/project claims are not accepted.

Credentials are authenticated-encrypted with project, environment, credential ID, and key ID bound into the ciphertext. Keep old key IDs in the keyring when selecting a new active key; removing them makes old credentials unreadable. APIs return references and metadata, never plaintext. The runtime resolves only declared references and masks their raw, URL-encoded, and base64 forms in outputs and failures.

## Checks

```sh
cargo test -p plugin-runtime -p environment-plugins --all-features --locked
cargo test -p server --bin server environment_extension::tests --locked
```

Database tests require disposable `ENVIRONMENT_PLUGIN_TEST_HOST_URL` and `ENVIRONMENT_PLUGIN_TEST_DATABASE_URL`. Run the ignored storage test first, then `environment_extension::tests::isolated_environment_plugin_api`. CI provisions and removes both databases. Browser checks are in `web/e2e/environment-plugins.spec.ts`.

## Database statements (T05)

The Database tab can run a saved, enabled profile after it is applied to the current environment. The API is `POST /api/environment-plugins/environments/{environment}/profiles/{profile}/execute`.

```json
{
  "query": "SELECT $1::bigint AS id, $2::text AS note",
  "parameters": [
    { "type": "integer", "value": 42 },
    { "type": "text", "value": null }
  ],
  "mode": "query",
  "maxRows": 100,
  "extract": [{ "variable": "record_id", "row": 0, "column": "id" }]
}
```

PostgreSQL uses `$1`, `$2` placeholders; MySQL 8 uses `?`. Parameters are bound by the driver and support `text`, `integer`, `float`, and `boolean`, including a typed `null`. Invalid fields, types, modes, and limits are rejected. Each statement owns its connection and transaction; query logging is disabled. Existing legacy probe requests retain their original behavior.

`query` is the default and runs in a read-only transaction. `execute` explicitly commits a write statement and reports `rowsAffected`; it does not support extraction. MySQL DDL follows the server's implicit-commit rules. Do not automatically retry writes after a timeout or connection failure: the server may already have committed them.

Results contain `columns` (name and SQL type), positional `rows`, `rowsAffected`, `truncated`, and `elapsedMs`. Positional rows preserve duplicate column names. Empty queries still return column metadata. Row limits default to 100, with a maximum of 1,000; a bounded prefix is explicitly marked `truncated`. Output size and execution deadlines are enforced, and server-side query time limits supplement the runtime deadline. Cancellation or a deadline drops the owned connection rather than returning it to a shared pool.

64-bit integers and decimals are returned as lossless strings; decimals are normalized without insignificant trailing zeros. Dates/timestamps and UUIDs are strings, binary values use hexadecimal, and JSON values retain their structure. Unsupported result types fail explicitly and can be cast to text in SQL. SQL NULL is distinct from a missing cell and appears as JSON null.

Extraction uses a zero-based row and column index or a unique column name. A required missing cell or ambiguous name fails the report with a named assertion; `required: false` skips a missing value. Extracted variables are returned in the report, with JSON null represented as `"null"`. They do not update host environment variables. Results and variables use the same credential redaction as connection tests. Reports can be downloaded as JSON without persisting query results in core business tables.

The MySQL provider's bounded query path targets MySQL 8; MariaDB is not covered by this adapter. CI requires real PostgreSQL and MySQL result tests, including parameter binding, precision, NULLs, empty results, limits, timeouts, and read/write transaction behavior.
