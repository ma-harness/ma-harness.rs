# Plugin Manifest Schema (P12-5 Registry v2)

> 业务方 `plugin.toml` / `plugin.json` 的公开 schema, 跟 `ma-harness-registry` 的 `PluginManifest` 对应.

## 字段

| 字段 | 类型 | 必填 | 说明 |
|---|---|---|---|
| `name` | string | ✅ | Plugin 名 (registry 内 unique) |
| `version` | semver | ✅ | 业务方写 "0.1.0", registry parse 成 `Version` |
| `description` | string | ✅ | 人类描述 |
| `author` | string | ✅ | 作者名 / email / org |
| `source` | object | ✅ | 来源 (Local / Git / Http) |
| `tags` | string[] |  | 业务方 search / filter 用 |

## Source 字段

### Local (v1 主推)

```json
{
  "type": "local",
  "path": "../path/to/plugin"
}
```

业务方 plugin 在 registry 同机, 直接 path 引用.

### Git (v2)

```json
{
  "type": "git",
  "url": "https://github.com/foo/bar",
  "rev": "v0.1.0"
}
```

`rev` 可选 (commit / tag / branch). 业务方 `mah plugin install` 时 git clone.

### Http (v2)

```json
{
  "type": "http",
  "url": "https://example.com/plugin.tar.gz"
}
```

业务方下载 tarball + 解压.

## 完整例子

```json
{
  "name": "my-plugin",
  "version": "0.1.0",
  "description": "My awesome plugin",
  "author": "user@example.com",
  "source": {
    "type": "local",
    "path": "./plugins/my-plugin"
  },
  "tags": ["utility", "fs", "v0.1"]
}
```

## Registry JSON file 格式

业务方 `mah plugin publish <plugin.json>` 写本地 registry, registry 整体序列化成 JSON:

```json
{
  "plugins": {
    "my-plugin": [
      {
        "name": "my-plugin",
        "version": "0.1.0",
        "description": "My plugin",
        "author": "user@example.com",
        "source": { "type": "local", "path": "./my-plugin" },
        "tags": ["utility"],
        "created_at": "2026-08-20T08:00:00Z",
        "updated_at": "2026-08-20T08:00:00Z"
      }
    ],
    "another-plugin": [ ... ]
  }
}
```

## v1 简化

- 仅 `Local` source, `Git` / `Http` 留 v2
- `created_at` / `updated_at` 业务方不用写, registry 自动填
- Plugin 签名 / 验证 留 P12-5 v2+

## 业务方使用

```rust
use ma_harness_registry::{Registry, PluginManifest, PluginSource};

// 业务方写 plugin.toml
let manifest: PluginManifest = toml::from_str(toml_content)?;

// 业务方 publish 到 registry
let mut reg = Registry::open("~/.ma-harness/registry.json")?;
reg.publish(manifest)?;
reg.save("~/.ma-harness/registry.json")?;

// 业务方导出 (GitHub Pages 静态站)
reg.export("docs/registry.json")?;
```

## 跟 dsh Registry 对比

| 维度 | dsh | ma-harness |
|---|---|---|
| Registry 后端 | npm + GitHub | JSON file v1, npm/GitHub v2 |
| 公开 schema | `plugin.toml` | `manifest-schema.md` (本文件) |
| 签名 / 验证 | ✅ | 留 v2 |
| Search | npm CLI | `search_by_tag` / `search_by_author` / `search_by_name` |

## 给后来人

- 改 schema 时, 同步改本文件 + `PluginManifest` Rust struct + 单元测试
- 业务方反馈 → PR 更新本 schema, 业务方 backward compat 优先
- 业务方 CI 跑 `cargo test --package ma-harness-registry` 全过 (18 + 7 = 25 tests)
