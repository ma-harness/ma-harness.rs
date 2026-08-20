# plugin.schema.json v1 — Design

[English](plugin-schema-v1.md) | [简体中文](zh-CN/plugin-schema-v1.md)

> **Purpose**: `plugin.toml` is the **human-written** plugin manifest;
> the JSON Schema is the **machine-readable validator**; the two must
> ship together.
> This design lands at
> `crates/ma_harness_plugin_schema/assets/plugin.schema.json`
> (generated end of Week 1); for now it lives in `docs/`.
>
> **Scope: metadata only**. Runtime config (model adapter endpoint /
> sandbox whitelist) goes into a separate `plugin.config.toml` or
> environment variables, see §11.

---

## 1. Design principles

| Principle | Description |
|-----------|-------------|
| **Human-written first** | `plugin.toml` is written by developers; **we must not require complex structures**. |
| **YAML 1.2** | Not TOML 0.5 (we use YAML because dsh uses YAML; ecosystem compatible). |
| **Machine-validated** | mah loads `plugin.schema.json` at startup and validates strictly; on failure → refuse to load + report error. |
| **Minimum fields** | Required fields ≤ 5; optional fields added on demand. **One fewer is better**. |
| **stable key** | Field names are snake_case and locked once v1 is released; new fields go through `v1_1` to avoid breaking compatibility. |
| **Cross-language friendly** | JSON Schema format; any language can read (not just Rust). |
| **Future-proof for hosted types** | When Phase 2 plugins use wasmtime / deno_core, this schema can be extended by adding a `runtime` field. |

---

## 2. plugin.toml example (user view)

```yaml
# plugins/ma_harness_plugin_bash/plugin.toml
schema_version: 1
name: bash
version: 0.1.0
description: Execute shell commands
authors:
  - yifenma <user@example.com>
license: MIT
entry: lib::BashPlugin
seam:
  tools:
    - run_command
  listeners: []
  services:
    - BashService
  commands: []
  handlers: []
sandbox:
  fs:
    read: []
    write: []
  net:
    egress: false
```

8 sections, **5 required + 3 optional**.

---

## 3. Field definitions

### 3.1 Required fields (5)

| Field             | Type    | Constraint                                  | Description                                  |
|-------------------|---------|---------------------------------------------|----------------------------------------------|
| `schema_version`  | integer | must be 1                                   | Version lock; future v2 goes through `v2`    |
| `name`            | string  | snake_case, ≤ 64 chars, globally unique     | Plugin ID, matches crate name                |
| `version`         | string  | semver 2.0                                  | 0.1.0, 1.2.3-alpha.1                         |
| `entry`           | string  | format `path::Type`                         | Entry struct path, e.g. `lib::BashPlugin`    |
| `seam`            | object  | see §4                                      | Plugin's declared capabilities               |

### 3.2 Optional fields (as needed)

| Field          | Type            | Default     | Description |
|----------------|-----------------|-------------|-------------|
| `description`  | string          | `""`        | One-sentence description; shown in `mah plugin list` |
| `authors`      | array of string | `[]`        | `"name <email>"` format, or just name |
| `license`      | string          | `"MIT"`     | SPDX identifier |
| `repository`   | string          | -           | URL |
| `keywords`     | array of string | `[]`        | For search |
| `sandbox`      | object          | see §5     | Sandbox requirements (mah configures landlock on demand) |
| `dependencies` | array of object | `[]`        | Other plugin dependencies, see §6 |
| `metadata`     | object          | `{}`        | User-defined, machine-readable, `mah plugin info` will print as-is |

---

## 4. `seam` field structure

```yaml
seam:
  tools: [string]      # function name, registers a tool via #[dsh_tool]
  listeners: [string]  # event name, registers via #[dsh_listener::on(Event::X)]
  services: [string]   # struct name, registers via #[dsh_service]
  commands: [string]   # command name, registers via #[dsh_command(name = "...")]
  handlers: [string]   # adapter name, registers via #[dsh_handler(adapter = "...")]
```

| Sub-field    | Type            | Description |
|--------------|-----------------|-------------|
| `tools`      | array of string | Function name (no path), mah looks up the registry at startup |
| `listeners`  | array of string | Event variant string, e.g. `SessionStart` |
| `services`   | array of string | struct name |
| `commands`   | array of string | Command name (the value of `name = "..."`) |
| `handlers`   | array of string | adapter ID |

**All default to `[]`; can all be empty (the plugin just declares it exists
without exposing capabilities).**

---

## 5. `sandbox` field structure (Phase 1 optional)

Phase 1 sandbox is Linux only (landlock). Other platforms accept the schema
but mah will warn-and-skip at startup.

```yaml
sandbox:
  fs:
    read: [absolute paths]      # whitelist of readable directories
    write: [absolute paths]     # whitelist of writable directories (read paths are implicit-readable)
  net:
    egress: false               # whether outbound network is allowed
  exec:
    enabled: true               # whether fork+exec is allowed (bash plugin needs true, fs plugin false)
    max_runtime_ms: 30000       # max runtime per fork+exec
```

| Sub-field         | Required? | Default |
|-------------------|-----------|---------|
| `sandbox`         | no        | `{}` (= all denied, strictest) |
| `sandbox.fs`      | no        | `{read: [], write: []}` |
| `sandbox.net`     | no        | `{egress: false}` |
| `sandbox.exec`    | no        | `{enabled: false, max_runtime_ms: 30000}` |

> **Design philosophy**: sandbox defaults to **all-deny** (deny by default);
> permissions must be explicitly declared to be granted.
> Consistent with dsh's "fail-closed", see
> `docs/ma-harness-arch-map.md` §4.

---

## 6. `dependencies` field structure

```yaml
dependencies:
  - name: cordis
    version: ">=0.1.0, <0.2.0"  # semver range
    optional: false
  - name: skill
    version: "^0.1.0"
    optional: true
```

| Sub-field   | Required? | Description |
|-------------|-----------|-------------|
| `name`      | yes       | Dependent plugin name, matches the other party's `name` field |
| `version`   | no        | semver range, default `"*"`, lenient |
| `optional`  | no        | Default `false`; `true` means mah doesn't force loading at startup |

> **Phase 1 simplification**: Phase 1 does not implement dependency resolution;
> this field is **recorded but mah only warns at startup, not enforces**.
> Phase 2 adds a full cargo-style resolver.

---

## 7. `metadata` field structure

```yaml
metadata:
  category: dev-tool        # free-form text; mah does not parse
  icon: 🐚                  # emoji
  homepage: https://...
  custom: 
    anything: goes
```

Completely open, machine-readable; `mah plugin info <name>` prints as-is.
**mah does not parse at startup, only validates JSON legality**.

---

## 8. JSON Schema body (YAML-form draft)

Lands as `crates/ma_harness_plugin_schema/assets/plugin.schema.json`,
draft of the schema:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://github.com/ma-harness/ma-harness.rs/schema/plugin.schema.json",
  "title": "ma-harness Plugin Manifest",
  "description": "plugin.toml schema v1",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "name", "version", "entry", "seam"],
  "properties": {
    "schema_version": {
      "type": "integer",
      "const": 1,
      "description": "Schema major version; locks compatibility"
    },
    "name": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9_]{0,63}$",
      "description": "Plugin ID, snake_case, globally unique"
    },
    "version": {
      "type": "string",
      "pattern": "^(0|[1-9]\\d*)\\.(0|[1-9]\\d*)\\.(0|[1-9]\\d*)(-[a-zA-Z0-9.-]+)?(\\+[a-zA-Z0-9.-]+)?$",
      "description": "semver 2.0"
    },
    "description": {
      "type": "string",
      "maxLength": 256
    },
    "authors": {
      "type": "array",
      "items": {
        "type": "string",
        "minLength": 1,
        "maxLength": 256
      },
      "maxItems": 32
    },
    "license": {
      "type": "string",
      "pattern": "^[A-Z0-9+-.]+$",
      "description": "SPDX identifier"
    },
    "repository": {
      "type": "string",
      "format": "uri"
    },
    "keywords": {
      "type": "array",
      "items": { "type": "string", "minLength": 1, "maxLength": 32 },
      "maxItems": 16,
      "uniqueItems": true
    },
    "entry": {
      "type": "string",
      "pattern": "^[a-zA-Z_][a-zA-Z0-9_]*::[a-zA-Z_][a-zA-Z0-9_]*$",
      "description": "path::Type format, e.g. lib::BashPlugin"
    },
    "seam": {
      "type": "object",
      "additionalProperties": false,
      "required": ["tools", "listeners", "services", "commands", "handlers"],
      "properties": {
        "tools":      { "type": "array", "items": { "type": "string", "pattern": "^[a-z][a-z0-9_]*$" }, "default": [] },
        "listeners":  { "type": "array", "items": { "type": "string", "pattern": "^[A-Z][A-Za-z0-9]*$" }, "default": [] },
        "services":   { "type": "array", "items": { "type": "string", "pattern": "^[A-Z][A-Za-z0-9]*$" }, "default": [] },
        "commands":   { "type": "array", "items": { "type": "string", "pattern": "^[a-z][a-z0-9_-]*$" }, "default": [] },
        "handlers":   { "type": "array", "items": { "type": "string", "pattern": "^[a-z][a-z0-9_-]*$" }, "default": [] }
      }
    },
    "sandbox": {
      "type": "object",
      "additionalProperties": false,
      "properties": {
        "fs": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "read":  { "type": "array", "items": { "type": "string", "pattern": "^/" }, "default": [] },
            "write": { "type": "array", "items": { "type": "string", "pattern": "^/" }, "default": [] }
          }
        },
        "net": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "egress": { "type": "boolean", "default": false }
          }
        },
        "exec": {
          "type": "object",
          "additionalProperties": false,
          "properties": {
            "enabled":        { "type": "boolean", "default": false },
            "max_runtime_ms": { "type": "integer", "minimum": 0, "maximum": 600000, "default": 30000 }
          }
        }
      }
    },
    "dependencies": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["name"],
        "properties": {
          "name":     { "type": "string", "pattern": "^[a-z][a-z0-9_]{0,63}$" },
          "version":  { "type": "string", "default": "*" },
          "optional": { "type": "boolean", "default": false }
        }
      },
      "maxItems": 32
    },
    "metadata": {
      "type": "object"
    }
  }
}
```

---

## 9. Validation flow (at mah startup)

```
mah start
  ↓
Load ~/.ma-harness/plugins/*/plugin.toml
  ↓
For each plugin.toml:
  1. Parse YAML → serde_yaml::Value
  2. Validate against plugin.schema.json (jsonschema crate)
  3. Failure → panic with friendly error ("plugin 'bash' schema validation failed: field 'name' does not match pattern")
  4. Validation passes → check name uniqueness (no duplicates across plugins)
  5. Check entry path exists (via proc-macro registry)
  6. Check that seam fields correspond to #[dsh_*] registered items at compile time (this is Week 2-3 work)
  ↓
Pass → ctx.plugin(Plugin::install(ctx))
```

---

## 10. Error message quality

Validation failures must give a **line-anchored YAML path**, not just
"validation failed":

```
error: plugin 'foo' validation failed
  --> plugins/foo/plugin.toml:12:5
   |
12 |     name: "MyPlugin"
   |     ^^^^^^^^^^^^^^^^^ Must match pattern ^[a-z][a-z0-9_]{0,63}$ (snake_case)
   |
help: Use snake_case, e.g. 'my_plugin'
```

Implementation: use `jsonschema` crate (Rust 0.17) to get
`ValidationError.instance_path` to look up the YAML source.

---

## 11. Configuration not in plugin.toml (runtime)

| Configuration | Location | Load timing |
|---------------|----------|-------------|
| Model API key | env var `MA_HARNESS_ADAPTER_<NAME>_API_KEY` | mah startup |
| Model endpoint | env var `MA_HARNESS_ADAPTER_<NAME>_ENDPOINT` | mah startup |
| Sandbox actual path whitelist | `~/.ma-harness/sandbox.toml` (per-plugin override) | mah startup |
| User-level default plugin list | `~/.ma-harness/plugins.yaml` | mah startup |
| Project-level plugin list | `<cwd>/.ma-harness/plugins.yaml` (project override) | mah startup |

**plugin.toml never contains secrets / endpoints / actual paths** ——
it follows the code into version control; those are deployment-time config.

---

## 12. Things we don't do (avoiding temptation)

| Don't do                                | Reason |
|-----------------------------------------|--------|
| Complex nested metadata validation      | metadata is open; only validate legal JSON, not semantics |
| plugin.toml cross-file references (`$ref`) | Single file, simple first |
| i18n fields                             | English-only; description field is English-only (i18n in Phase 3) |
| plugin.toml in TOML/JSON                | Lock YAML 1.2; align with dsh |
| Plugin signing (GPG)                    | Internal repo, skip for now; add before Phase 2 public release |
| Plugin marketplace / remote install    | Internal repo, install via `git clone`; Phase 2 |

---

## 13. First landing instance (end of Week 1)

`crates/ma_harness_plugin_schema/assets/plugin.schema.json` lands as the
JSON in §8 above, plus `plugin.toml` for the 6 first-party plugins
(the 6 from arch-map §6) committed together.

Example plugin.toml:

```yaml
# plugins/ma_harness_plugin_bash/plugin.toml
schema_version: 1
name: bash
version: 0.1.0
description: Execute shell commands (sandbox-restricted)
authors:
  - ma-harness contributors
license: MIT
entry: lib::BashPlugin
seam:
  tools: [run_command]
  services: [BashService]
sandbox:
  exec:
    enabled: true
    max_runtime_ms: 30000
  fs:
    write: []
  net:
    egress: false
```

```yaml
# plugins/ma_harness_plugin_fs/plugin.toml
schema_version: 1
name: fs
version: 0.1.0
description: File system read / write
entry: lib::FsPlugin
seam:
  tools: [read_file, write_file, list_dir]
  seam_handlers: []
```

(The other 4 are similar; bash / fs / web / subagent / skill / cordis each
have one.)

---

## 14. Changelog

| Date       | Change |
|------------|--------|
| 2026-08-18 | Initial version, plugin.schema.json v1 full design, with 6 first-party plugin.toml drafts |
| 2026-08-20 | Updated `$id` to GitHub mirror (was Gitee). Note: P11-6 Plugin Registry adds `PluginManifest` runtime type parallel to this static schema. |
