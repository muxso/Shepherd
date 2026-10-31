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

Credentials are authenticated-encrypted with project, environment, credential ID, and key ID bound into the ciphertext. Keep old key IDs in the keyring when selecting a new active key; removing them makes old credentials unreadable. APIs return references and metadata, never plaintext. The runtime resolves only declared references and masks their raw, URL-encoded, base64, and hexadecimal forms in outputs and failures.

## Checks

```sh
cargo test -p plugin-runtime -p environment-plugins --all-features --locked
cargo test -p server --bin server environment_extension::tests --locked
```

API tests require disposable `ENVIRONMENT_PLUGIN_TEST_HOST_URL`, `ENVIRONMENT_PLUGIN_TEST_DATABASE_URL`, and `REDIS_URL`. Storage-only tests require the two database URLs. Run the ignored storage test first, then `environment_extension::tests::isolated_environment_plugin_api`. CI provisions and removes both databases. Browser checks are in `web/e2e/environment-plugins.spec.ts`.

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

## Redis profiles (T06)

Profiles provide host/port, database (default 0), optional ACL username, password references, TLS (default `require`), and protocol (`resp2` by default). RESP3 requires a compatible server. Set TLS to `disable` for plaintext test servers. TLS uses rustls and Mozilla roots with hostname/certificate verification; custom CA profiles and insecure verification are not exposed. A username requires a credential reference.

The Redis tab saves/tests profiles, executes commands through the scoped execution API, extracts variables, and downloads reports:

```json
{
  "command": "MGET",
  "arguments": ["key with spaces", "missing-key"],
  "extract": [
    { "variable": "first", "pointer": "/0" },
    { "variable": "second", "pointer": "/1", "required": false }
  ]
}
```

Each argument is a complete string or binary object such as `{"hex":"00ff"}`. Empty strings, spaces, and Unicode are preserved. Legacy probe requests retain their existing behavior.

Reports contain `kind`, normalized `value`, and `elapsedMs`. Integers use strings; UTF-8 replies are text, other bytes use `{"hex":"..."}`, null remains JSON null, arrays/sets are arrays, and maps are arrays of key/value pairs. This preserves duplicate/binary map keys. Extraction uses JSON Pointer against `value`: an empty pointer selects the full value, `/0` the first array item, and `/0/1` the first map pair's value. Required missing paths fail a named assertion; a present null is extracted as `"null"`. Variables stay in the report and do not modify host variables. Credential redaction covers results, variables, and errors.

Each call owns its multiplexed connection and driver task; timeout/cancellation drops the connection. This does not roll back a command already executed by Redis. Do not automatically retry writes after a timeout. Input is limited to 256 arguments, 64 KiB per decoded argument, and 1 MiB total; replies enforce the output budget, 10,000 nodes, and 64 nesting levels. Oversized results fail explicitly.

Authentication, database selection, and protocol negotiation belong to the profile. The single-command interface rejects connection-management, transaction, streaming, replication, and cluster-session commands: AUTH, SELECT, HELLO, RESET, QUIT, CLIENT, MULTI/EXEC/DISCARD/WATCH/UNWATCH, subscription commands, MONITOR, SYNC/PSYNC/REPLCONF, READONLY/READWRITE/ASKING. Stateful sessions, subscriptions, cluster routing, and shared runner integration remain separate work.

CI requires real Redis command tests (`probe --test redis_commands`) with a disposable Redis 6+ fixture: ACL authentication, RESP3, spaced/binary arguments, precision, nulls, output limits, timeout cleanup, wrong credentials, and server failures. The isolated host API test also verifies saved Redis profiles, encrypted credentials, permissions, extraction, and redaction. TLS URL construction is unit-tested; trusted TLS endpoint tests are not part of this fixture.

## SSH profiles (T07)

SSH profiles require host, username, and a verified SHA256 host fingerprint; port defaults to 22. Obtain the fingerprint through a trusted server administrator. There is no accept-all or automatic trust path. Connection tests verify the host key and authentication without running a command.

Authentication defaults to `private_key`; `password` mode must be selected explicitly. The selected mode requires its corresponding encrypted credential reference, and modes cannot be mixed. Private keys use the OpenSSH format, including encrypted keys with a separate passphrase reference. Keys are limited to 16 KiB and bcrypt KDF costs to 1–64 rounds; unsupported formats/costs fail explicitly. Multiline key forms preserve newlines, submit through the credential API, and clear after saving. Profile values never contain private-key plaintext.

Execute through the saved, enabled connection binding:

```json
{ "command": "printf 'hello\\n'; printf 'diagnostic\\n' >&2; exit 7" }
```

Reports separate stdout and stderr with explicit `utf8` or `hex` encoding, record the verified host fingerprint and elapsed time, and distinguish exit codes, signals, and missing termination status. Only exit code 0 succeeds. A missing exit status never defaults to success. The UI renders both streams and exports the structured report. Outputs, errors, and encoded binary values use the common credential redaction.

The optional profile `timeout_ms` can reduce the host execution deadline, never extend it. Combined capture and serialized report limits are enforced. A socket guard shuts down the underlying transport on completion, timeout, cancellation, or error, including failed handshakes. Closing SSH does not guarantee that a process already started on the remote host has stopped; do not automatically retry writes after a timeout.

Compatibility: legacy `ssh` probes now require `metadata.host_key_sha256` alongside `user` and `password`; unverified requests fail before authentication. Legacy and profile execution share the same pinned, bounded transport. Legacy output remains combined text (binary data is marked `hex:`), while profiles retain separate streams. Private-key authentication belongs to profiles; agents, certificates, jump hosts, tunnels, PTYs, stdin, and persistent sessions are not supported by this provider. Shared runner configuration remains T17.

CI requires loopback SSH protocol tests with throwaway keys generated by `ssh-keygen`: pin rejection before authentication, password/plain/encrypted-key authentication, stream separation, nonzero/missing/signal exits, binary output, limits, timeouts, and connection cleanup. Provider and isolated host API tests cover encrypted credential references, permissions, redaction, persistence, and host database isolation. Browser checks cover multiline key handling and command reports.

## gRPC profiles (T08)

Profiles contain an `http://host:port` (cleartext) or `https://host:port` endpoint and a `proto_files` JSON object mapping relative `.proto` names to source strings. Definitions compile in memory; imports resolve only from this object and bundled Google well-known types. Filesystem/network imports and replacement of the `google.protobuf` namespace are rejected. Limits: 32 files, 256 KiB source, 256 methods. The authenticated, project-scoped `GET .../profiles/{id}/metadata` endpoint lists method signatures and optional request examples without opening a remote connection or resolving credentials. Streaming methods are visible but explicitly unsupported.

The gRPC tab selects a unary method, edits canonical Protobuf JSON, invokes it through the existing binding/authorization pipeline, and exports the redacted report. Example execute input:

```json
{
  "method": "/example.Echo/Call",
  "message": {"count": "9007199254740993", "data": "AP8="},
  "metadata": {"x-tenant": "example", "trace-bin": "AP8="}
}
```

64-bit protobuf integers retain string precision; bytes use Base64 and well-known types use Protobuf JSON representations. Unknown message fields, unknown methods, and streaming calls fail before network I/O. Public metadata names must be lowercase; per-call values override profile public values. `-bin` values use Base64. Protocol-reserved headers are rejected. Authentication headers (`authorization`, `proxy-authorization`, `cookie`, `x-api-key`) cannot be supplied as public metadata. Use the encrypted `credential` reference and optional `credential_name` (default `authorization`); the credential contains the complete value, for example `Bearer ...`. Credentials cannot be overridden by call metadata and are marked sensitive in the transport.

TLS verifies the server name and public trust roots by default. Optional `server_name` selects a verified TLS name; `ca_certificate` replaces public roots with a private CA. Optional `client_certificate` and `client_key` references must be supplied together for mutual TLS. All certificate/key values use the multiline encrypted credential workflow. TLS settings are invalid for cleartext endpoints. Connection tests check transport/TLS only; application authorization is verified by invoking a method.

Reports preserve the actual gRPC status code/message, optional Base64 status details, protobuf response, received response metadata, and elapsed time. Non-OK statuses fail `grpc.status` even when HTTP transport returned 200. Each operation owns one connection; completion, errors, cancellation, and deadlines close its socket and do not retry writes. Closing a connection does not undo remote side effects. The smaller of profile/system deadlines applies; request protobuf bytes, response decoding, and the final serialized report enforce output limits.

Native loopback tests require no `protoc` or external service. They verify protobuf precision/well-known types, binary metadata, real TLS/mTLS, name/trust/identity rejection, unknown fields, non-OK status, limits, deadlines, and socket cleanup. The isolated host API test verifies saved credential references, viewer metadata access, execution denial, cross-project denial, redaction, and absence of plugin tables in the host database. Browser checks cover method inspection without invocation, request editing, status failures, and report download. Shared debug/case/scenario/plan resolution remains T17.

## HOST overrides (T09)

The HOST provider stores `addresses` only in the plugin database:

```json
{"example.test": ["192.0.2.10", "2001:db8::10"]}
```

Bind the profile to the `host` slot and select that environment for a server-side HTTP debug request. The debug API accepts `environmentId`; the normal request editors forward the selected environment. Resolution requires execution permission and project membership, checks environment enablement, and uses only enabled HOST bindings/profiles. Conflicting domains across profiles fail before sending any request. Disablement/unbinding takes effect on subsequent executions. Runtime plugin disablement restores native resolution.

Rules accept at most 128 DNS names with 1–16 distinct IPv4/IPv6 addresses each. DNS names are normalized to lowercase/IDNA; IP-literal names, wildcards, ports, paths, trailing dots, duplicate normalized names, and empty lists are rejected. Overrides replace only DNS answers. The URL, explicit URL port, HTTP authority, TLS SNI, certificate verification, headers, and redirect semantics remain those of the original request. Each execution owns its client/resolver settings; overrides never modify `/etc/hosts`, the process resolver, or another environment. These clients bypass environment proxies so the proxy cannot silently bypass an override. Overridden names do not trigger the runner's diagnostic DNS lookup (`dnsMs: 0`). Other names use normal DNS.

The UI explicitly limits this support to server-side HTTP debugging. Browser direct execution cannot apply custom DNS; batch cases and full scenario/test-plan runs await the shared pipeline in T17. Non-HTTP protocols retain their existing transport settings.

Native tests verify real HTTP authority, HTTPS SNI, certificate-name rejection, and resolver isolation. The isolated API fixture saves/binds rules, invokes a real HTTP service through an otherwise unresolvable name, rejects viewers/cross-project access and conflicting bindings, and verifies host database isolation. Browser checks verify save/reload and the HOST binding slot.
