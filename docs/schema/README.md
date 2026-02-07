# JSON Schemas

The `docs/schema` directory contains self-contained JSON Schema Draft-07
documents that describe the machine-readable output of the CLI commands:

| Schema | Command | Description |
| ------ | ------- | ----------- |
| `info.schema.json` | `neo-decompiler info --format json` | NEF metadata, manifest summary, method tokens (includes `returns_value`), warnings. |
| `disasm.schema.json` | `neo-decompiler disasm --format json` | Instruction stream including operand types/values and optional `returns_value` for `SYSCALL`. |
| `decompile.schema.json` | `neo-decompiler decompile --format json` | High-level/pseudocode listings plus the disassembly (including optional `returns_value`), manifest summary, method tokens (`returns_value`), and analysis (`call_graph`, `xrefs`, `types`). |
| `tokens.schema.json` | `neo-decompiler tokens --format json` | Standalone method-token listing (with `returns_value`). |

## Versioning

- Schemas follow semantic versioning at the file level. Breaking changes bump
  the file name (for example: `info.schema.v2.json`). The current files describe
  version `1`.
- Consumers should pin to a specific commit or release tag to guarantee stability.
- The Rust test suite (`tests/cli_smoke.rs`) validates the CLI output against the
  checked-in schemas so regressions are caught during CI.
- The CLI can list schemas via `neo-decompiler schema --list` (each entry includes
  the semantic version, path, and a short description) or `neo-decompiler schema --list-json`
  for machine-readable listings, and print one via
  `neo-decompiler schema <info|disasm|decompile|tokens>`. Combine `--json-compact`
  to strip whitespace or `--output schema.json` to persist the file, ensuring
  deterministic access without cloning the repository. Add `--validate path/to.json`
  (or `--validate -` to read from stdin) to verify that a JSON report matches the
  selected schema, and combine `--no-print` to suppress the schema body during validation.

## Validating output

### Rust example

```rust
use jsonschema::validator_for;
use serde_json::Value;

fn validate(output: &str, schema_json: &Value) {
    let schema = validator_for(schema_json).expect("invalid schema");
    let value: Value = serde_json::from_str(output).expect("invalid JSON");
    schema.validate(&value).expect("schema mismatch");
}
```

### CLI example (Node.js / ajv-cli)

```bash
npm install -g ajv-cli
neo-decompiler info --format json hello.nef > report.json
ajv validate -s docs/schema/info.schema.json -d report.json
```

### jq quick check

If you only need to ensure the schema file is syntactically correct, run:

```bash
jq empty docs/schema/info.schema.json
```

## Extending the schemas

When adding new JSON fields to any CLI command:

1. Update the relevant schema file in this directory.
2. Modify `README.md` if the change affects public documentation.
3. Extend the smoke tests in `tests/cli_smoke.rs` to assert the new data and keep
   the schema validation coverage high.
