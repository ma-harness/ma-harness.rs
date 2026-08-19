# mah-py — Python SDK for ma-harness.rs

> **v0.1.0** — 同步版 (subprocess 调 `mah run`)

Python SDK for [ma-harness.rs](https://gitee.com/yifenma/ma-harness.rs) (Rust AI agent orchestrator). 让 Python 业务方能用 ma-harness 跑 agent, 跟 dsh Python SDK (`deepseek-harness-sdk`) 对齐 API 风格.

## 安装 (本地, 暂未发 PyPI)

```bash
cd crates/mah-py
pip install -e .
```

需要 `mah` binary 在 PATH 或 `MAH_PATH` 环境变量指向.

## 5 行入门

```python
from mah_py import Mah

with Mah() as m:
    result = m.run("Say hi.")
    print(result.content)
```

## 高级用法

### 指定 model

```python
import os
os.environ["OPENAI_API_KEY"] = "sk-..."

with Mah(model="openai:gpt-4o-mini") as m:
    r = m.run("Explain quantum entanglement in 50 words")
    print(f"tokens: {r.prompt_tokens} + {r.completion_tokens}")
```

### 多轮对话 (指定 session)

```python
with Mah() as m:
    r1 = m.run("My name is Alice", session="chat-1")
    r2 = m.run("What's my name?", session="chat-1")
    # r2 会基于 r1 上下文
```

### 跑 conformance

```python
with Mah() as m:
    summary = m.conformance(
        fixtures="crates/ma-harness-conformance/fixtures/dsh_synthetic.jsonl",
        dsh=True,
        output="D:/tmp/mah-py-conformance",
    )
    print(summary)
```

## API

### `Mah(mah_path=None, model="stub", timeout=60.0)`

| 字段 | 类型 | 说明 |
|---|---|---|
| `mah_path` | str \| None | `mah` binary 路径 (None = `PATH` / `MAH_PATH`) |
| `model` | str | 默认 model (`"stub"`, `"openai:gpt-4o-mini"`, `"anthropic:claude-3-5-sonnet"` 等) |
| `timeout` | float | subprocess timeout (秒) |

### `m.run(message, session=None, model=None) -> RunResult`

跑一次 agent (同步), 走 `mah run` 命令.

| 字段 | 类型 | 说明 |
|---|---|---|
| `message` | str | 用户消息 |
| `session` | str \| None | session ID (None = 新建) |
| `model` | str \| None | 覆盖默认 model |

Returns `RunResult { session_id, run_id, content, prompt_tokens, completion_tokens }`.

### `m.version() -> str`

返回 `mah` 版本 (e.g. `"mah 0.1.0"`).

### `m.conformance(fixtures, dsh=False, output=None, verbose=False) -> str`

跑 conformance fixture, 返回 `mah conformance` stdout 摘要.

## Examples

5 个 example 在 `examples/` 目录:
- `01_hello.py` — 最小 hello world
- `02_with_model.py` — 用真 LLM
- `03_session.py` — 多轮对话
- `04_conformance.py` — 跑 conformance
- `05_error_handling.py` — 错误处理

```bash
python examples/01_hello.py
```

## 设计 (v1 简化)

- **v1 (现在)**: 同步 subprocess 调 `mah run` / `mah conformance`
  - 优: 实现简单, 1-2 周可发, 不需要长连接管理
  - 缺: 每次调用 spawn 新进程, 无流式输出
- **v2 (计划)**: 走 `mah run-stream` (gRPC streaming) + 流式 token
  - 优: 真流式 (跟 dsh SDK `result.text` 增量读齐)
  - 缺: 需要起 server (gRPC + HTTP), 业务方要先 `mah start`

## 跟 dsh Python SDK 对比

| 维度 | dsh (`deepseek-harness-sdk`) | mah-py (`mah-py`) |
|---|---|---|
| Python 包 | `deepseek-harness-sdk` (PyPI) | `mah-py` (本地) |
| import | `from deepseek_harness import DeepSeekHarness` | `from mah_py import Mah` |
| 协议 | JSON-RPC stdio (`dsh-jsonrpc-agent`) | subprocess `mah` CLI |
| 同步 / 异步 | 同步 (subprocess) | 同步 (subprocess) |
| 流式 | v1 没有 | v1 没有 (v2 gRPC) |
| 安装 | pip install (含 binary wheel) | pip install (binary 单独装) |

## 测试

```bash
# 内部联调 (subprocess 调 mah)
cd crates/mah-py
pytest tests/ -v
```

## 路线图

- v0.1 (现在): subprocess `mah run` 同步
- v0.2: `mah run-stream` 流式
- v0.3: async / await API
- v0.4: type stub (mypy --strict)
- v0.5: PyPI 发布 (`pip install mah-py`)

## 仓库

- 主仓库: https://gitee.com/yifenma/ma-harness.rs
- mah CLI: `crates/ma-harness-cli/`
- 设计参考: dsh Python SDK (`deepseek-harness-sdk`)

## License

MIT
