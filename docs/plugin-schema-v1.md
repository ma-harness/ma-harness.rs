# plugin.schema.json v1 — 设计

> 目的: `plugin.toml` 是**人手写**的插件清单,JSON Schema 是**机读校验**,两者必须配套。
> 本设计落到 `crates/ma_harness_plugin_schema/assets/plugin.schema.json`(Week 1 末生成),
> 现在先 spec 在 `docs/`。
>
> 范围:**仅元数据**。运行时配置(模型 adapter endpoint / sandbox 白名单)走另一份
> `plugin.config.toml` 或环境变量,见 §11。

---

## 1. 设计原则

| 原则 | 说明 |
|---|---|
| **人手写优先** | `plugin.toml` 是开发者写的,**不能要求填复杂结构**。 |
| **YAML 1.2** | 不是 TOML 0.5(我们用 YAML,因为 dsh 用 YAML,生态兼容)。 |
| **机读校验** | mah 启动时 load `plugin.schema.json` 严格校验,失败 → 拒绝加载 + 报错。 |
| **字段最少** | 必须字段 ≤ 5 个,可选字段按需加。**能少一个就少一个**。 |
| **stable key** | 字段名 snake_case 且一旦发布 v1 锁住,加新字段走 `v1_1` 不破坏兼容。 |
| **跨语言友好** | JSON Schema 格式,任何语言都能读(不只是 Rust)。 |
| **未来能 host 类型信息** | Phase 2 插件用 wasmtime / deno_core 时,本 schema 加 `runtime` 字段即可扩展。 |

---

## 2. plugin.toml 例子 (用户视角)

```yaml
# plugins/ma_harness_plugin_bash/plugin.toml
schema_version: 1
name: bash
version: 0.1.0
description: 执行 shell 命令
authors:
  - yifenma <yifenma@example.com>
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

8 个 section,**5 个必填 + 3 个选填**。

---

## 3. 字段定义

### 3.1 必填字段 (5)

| 字段 | 类型 | 约束 | 说明 |
|---|---|---|---|
| `schema_version` | integer | 必须是 1 | 锁版本,未来 v2 走 `v2` 字段 |
| `name` | string | snake_case, ≤ 64 字符, 全局唯一 | 插件 ID,跟 crate 名一致 |
| `version` | string | semver 2.0 | 0.1.0, 1.2.3-alpha.1 |
| `entry` | string | 格式 `path::Type` | 入口 struct 路径,例如 `lib::BashPlugin` |
| `seam` | object | 见 §4 | 插件声明的能力 |

### 3.2 选填字段 (按需)

| 字段 | 类型 | 默认 | 说明 |
|---|---|---|---|
| `description` | string | `""` | 一句话说明,会显示在 `mah plugin list` |
| `authors` | array of string | `[]` | `"name <email>"` 格式,或仅 name |
| `license` | string | `"MIT"` | SPDX identifier |
| `repository` | string | - | URL |
| `keywords` | array of string | `[]` | 检索用 |
| `sandbox` | object | 见 §5 | sandbox 需求声明(mah 启动时按需配置 landlock) |
| `dependencies` | array of object | `[]` | 其他插件依赖,见 §6 |
| `metadata` | object | `{}` | 用户自定义,机读,`mah plugin info` 会原样打印 |

---

## 4. `seam` 字段结构

```yaml
seam:
  tools: [string]      # 函数名,对应 #[dsh_tool] 注册的工具
  listeners: [string]  # 事件名,对应 #[dsh_listener::on(Event::X)]
  services: [string]   # struct 名,对应 #[dsh_service] 的类型
  commands: [string]   # 命令名,对应 #[dsh_command(name = "...")]
  handlers: [string]   # adapter 名,对应 #[dsh_handler(adapter = "...")]
```

| 子字段 | 类型 | 说明 |
|---|---|---|
| `tools` | array of string | 函数名(不带路径),mah 启动时反射查 crate 注册表 |
| `listeners` | array of string | 事件 variant 字符串,例如 `SessionStart` |
| `services` | array of string | struct 名 |
| `commands` | array of string | 命令名(就是 `name = "..."` 的引号内容) |
| `handlers` | array of string | adapter ID |

**全部默认 `[]`,可以全部空(插件只是声明自己存在,不暴露能力)。**

---

## 5. `sandbox` 字段结构 (Phase 1 选填)

Phase 1 sandbox 只 Linux (landlock)。其他平台 schema 接受但 mah 启动时 warn-and-skip。

```yaml
sandbox:
  fs:
    read: [绝对路径]      # 允许读的目录白名单
    write: [绝对路径]     # 允许写的目录白名单 (read 路径自动 implicit read)
  net:
    egress: false         # 是否允许出站网络
  exec:
    enabled: true         # 是否允许 fork+exec (bash 插件要 true, fs 插件 false)
    max_runtime_ms: 30000 # 单次 fork+exec 最长跑多久
```

| 子字段 | 必填? | 默认 |
|---|---|---|
| `sandbox` | 否 | `{}` (=全拒绝,最严) |
| `sandbox.fs` | 否 | `{read: [], write: []}` |
| `sandbox.net` | 否 | `{egress: false}` |
| `sandbox.exec` | 否 | `{enabled: false, max_runtime_ms: 30000}` |

> **设计哲学**: sandbox 默认**全拒绝** (deny by default),要开权限必须显式声明。
> 跟 dsh 的 "fail-closed" 一致,见 `docs/ma-harness-arch-map.md` §4。

---

## 6. `dependencies` 字段结构

```yaml
dependencies:
  - name: cordis
    version: ">=0.1.0, <0.2.0"  # semver range
    optional: false
  - name: skill
    version: "^0.1.0"
    optional: true
```

| 子字段 | 必填? | 说明 |
|---|---|---|
| `name` | 是 | 依赖的插件名,跟对方 `name` 字段一致 |
| `version` | 否 | semver range,默认 `"*"`,宽松 |
| `optional` | 否 | 默认 `false`,true = mah 启动不强制加载 |

> **Phase 1 简化**: Phase 1 不实现依赖解析,本字段先**记录在案但 mah 启动时只 warn 不 enforce**。
> Phase 2 加完整 cargo-style 解析器。

---

## 7. `metadata` 字段结构

```yaml
metadata:
  category: dev-tool        # 自由文本, mah 不解析
  icon: 🐚                  # emoji
  homepage: https://...
  custom: 
    anything: goes
```

完全开放,机读,`mah plugin info <name>` 会原样打印。**mah 启动时不解析,只校验 JSON 合法**。

---

## 8. JSON Schema 主体 (YAML 形式的草案)

落地成 `crates/ma_harness_plugin_schema/assets/plugin.schema.json`,下面是 schema 草案:

```json
{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "$id": "https://gitee.com/yifenma/ma-harness.rs/schema/plugin.schema.json",
  "title": "ma-harness Plugin Manifest",
  "description": "plugin.toml schema v1",
  "type": "object",
  "additionalProperties": false,
  "required": ["schema_version", "name", "version", "entry", "seam"],
  "properties": {
    "schema_version": {
      "type": "integer",
      "const": 1,
      "description": "schema 主版本号,锁住兼容性"
    },
    "name": {
      "type": "string",
      "pattern": "^[a-z][a-z0-9_]{0,63}$",
      "description": "插件 ID,snake_case,全仓唯一"
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
      "description": "path::Type 格式,例如 lib::BashPlugin"
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

## 9. 校验流程 (mah 启动时)

```
mah start
  ↓
加载 ~/.ma-harness/plugins/*/plugin.toml
  ↓
对每个 plugin.toml:
  1. 解析 YAML → serde_yaml::Value
  2. 跟 plugin.schema.json 校验 (jsonschema crate)
  3. 失败 → panic with 友好错误 ("plugin 'bash' schema 校验失败: field 'name' 不匹配 pattern")
  4. 校验通过 → 检查 name 唯一 (跨插件不重名)
  5. 检查 entry 路径存在 (通过 proc-macro 注册表查)
  6. 检查 seam 字段对应的 #[dsh_*] 注册项在编译期已注册 (这一步 Week 2-3 做)
  ↓
通过 → ctx.plugin(Plugin::install(ctx))
```

---

## 10. 错误信息友好度

校验失败要给出**指向行号的 YAML 路径**,不是只说"校验失败":

```
error: plugin 'foo' 校验失败
  --> plugins/foo/plugin.toml:12:5
   |
12 |     name: "MyPlugin"
   |     ^^^^^^^^^^^^^^^^^ 必须匹配 pattern ^[a-z][a-z0-9_]{0,63}$ (snake_case)
   |
help: 改成 snake_case, 例如 'my_plugin'
```

实现:用 `jsonschema` crate (Rust 0.17) 拿 `ValidationError.instance_path` 反查 YAML 源文件。

---

## 11. 不在 plugin.toml 的配置 (运行时)

| 配置 | 落点 | 加载时机 |
|---|---|---|
| 模型 API key | 环境变量 `MA_HARNESS_ADAPTER_<NAME>_API_KEY` | mah 启动时 |
| 模型 endpoint | 环境变量 `MA_HARNESS_ADAPTER_<NAME>_ENDPOINT` | mah 启动时 |
| Sandbox 实际路径白名单 | `~/.ma-harness/sandbox.toml` (per-plugin override) | mah 启动时 |
| 用户级默认 plugin 列表 | `~/.ma-harness/plugins.yaml` | mah 启动时 |
| 项目级 plugin 列表 | `<cwd>/.ma-harness/plugins.yaml` (项目级覆盖) | mah 启动时 |

**plugin.toml 永远不出现 secrets / endpoints / 实际路径** ——
它跟随代码进版本,这些是部署时配置。

---

## 12. 不做的事 (避免诱惑)

| 不做 | 理由 |
|---|---|
| 复杂嵌套的 metadata 校验 | metadata 是开放字段,只校验合法 JSON,不校验语义 |
| plugin.toml 跨文件引用 (`$ref`) | 单文件,简单优先 |
| 国际化 (i18n) 字段 | 全英,描述字段也只支持英文 (国际化 Phase 3) |
| plugin.toml 支持 TOML/JSON | 锁 YAML 1.2 一种,跟 dsh 对齐 |
| 插件签名 (GPG) | 内部仓库,先不上,Phase 2 公开发布前加 |
| 插件 marketplace / 远程安装 | 内部仓库,直接 `git clone` 装,Phase 2 |

---

## 13. 第一个落地实例 (Week 1 末)

`crates/ma_harness_plugin_schema/assets/plugin.schema.json` 落成上面 §8 的 JSON,
配套 6 个 first-party 插件的 `plugin.toml` 一起 commit (arch-map §6 的 6 个)。

每个 plugin.toml 例子:

```yaml
# plugins/ma_harness_plugin_bash/plugin.toml
schema_version: 1
name: bash
version: 0.1.0
description: 执行 shell 命令 (受 sandbox 限制)
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
description: 文件系统读 / 写
entry: lib::FsPlugin
seam:
  tools: [read_file, write_file, list_dir]
seam_handlers: []
```

(其他 4 个类似,bash / fs / web / subagent / skill / cordis 各一份。)

---

## 14. 变更记录

| 日期 | 变更 |
|---|---|
| 2026-08-18 | 初版, plugin.schema.json v1 完整设计, 含 6 个 first-party plugin.toml 草案 |
