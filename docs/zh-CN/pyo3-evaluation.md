# ma-harness pyo3 Native Binding 评估 (P5-9 / Day 98)

> **状态**: 研究性质, 评估是否值得在现有 gRPC binding 之上, 加 pyo3 native binding
> **结论**: 暂缓. 详见 § 4 决策矩阵



[English](../pyo3-evaluation.md) — coming soon. 中文为主.


## 1. 背景

### 1.1 当前 Python 业务方接入路径

ma-harness 现有 2 种 Python 接入方式:

| 方式 | 路径 | 依赖 |
|---|---|---|
| **gRPC binding** | `bindings/python/` | grpcio + grpcio-tools (业务方装) |
| **HTTP fetch** | docs/api/openapi.json | requests / aiohttp (业务方装) |

gRPC 路径示例 (4 步):
```python
import grpc
from ma_harness_pb2 import agent_pb2, agent_pb2_grpc

channel = grpc.insecure_channel("localhost:50051")
stub = agent_pb2_grpc.AgentServiceStub(channel)
response = stub.Run(agent_pb2.RunRequest(session_id="s1", user_message="hi", ...))
```

### 1.2 业务方痛点 (假设)

- **网络开销**: gRPC 走 TCP, 微秒级延迟, 业务方高 QPS 场景累加可观
- **proto 编译**: 业务方每次 proto 变更要重跑 `compile_proto.py` 生成 stub
- **服务依赖**: 业务方必须起 `mah start` server 才能跑测试, 单测 setup 复杂
- **部署耦合**: 业务方 Python 进程跟 Rust server 进程分离, K8s pod 调度多 1 个

### 1.3 pyo3 是什么

`pyo3` 是 Rust ↔ Python 双向绑定的 crate:
- Rust 函数用 `#[pyfunction]` 暴露给 Python
- Python 类用 `#[pyclass]` 包装 Rust struct
- 编译产物是 `*.so`/`.pyd` (Linux/macOS/Windows 原生模块)
- 业务方 `pip install` 后 `import ma_harness_native` 直接调, **不走网络**

## 2. pyo3 接入方案

### 2.1 走法 A: 完整 in-process binding

业务方用 `import ma_harness_native as mh` 直接调, 不需要 Rust server:

```python
# 业务方代码
import ma_harness_native as mh

# 创建 session
session = mh.Session.create(name="my-app", user_id="alice")
# print(session.id)  # "abc-123"

# 跑 agent (StubModelAdapter, 真 LLM 走 HTTP adapter 内部调)
response = mh.Agent.run(
    session_id=session.id,
    user_message="hello",
    model="openai:gpt-4o-mini",
    api_key="sk-...",
)
# print(response.content)  # "Hi there!"
```

实现:
- `ma-harness-py` 独立 crate, `crate-type = ["cdylib"]`
- `ma-harness-core::AgentLoop` + `EventLog` 直接调, 不走 tonic gRPC
- OpenAI / Anthropic adapter 内嵌 (已有 `reqwest` 依赖)
- 业务方: `pip install ma-harness` 自动从 PyPI 拉 wheel (含 .so)

### 2.2 走法 B: gRPC server embedded

业务方装 native package, 但内部仍跑 tonic gRPC server (in-process, 0.0.0.0:0), 客户端走 channel 调:

```python
import ma_harness_native as mh
# 内部 fork thread 跑 tonic server, 拿 random port
client = mh.GRPCClient.connect()  # 走 stub
response = client.Agent.run(...)
```

实现:
- 复用现有 tonic stub, 业务方用相同 API
- 进程内 fork (省去网络), 但还是走 gRPC 序列化

### 2.3 走法 C: hybrid (默认 native, fallback gRPC)

```python
import ma_harness_native as mh

# 默认 in-process (pyo3)
if mh.is_native_available():
    response = mh.Agent.run(...)
else:
    # fallback gRPC (没装 native package)
    import grpc
    # ...
```

## 3. 评估维度

### 3.1 性能

| 指标 | gRPC binding | pyo3 in-process |
|---|---|---|
| 启动延迟 | 1-2ms (TCP handshake) | <0.1ms (直接调) |
| 单次 RPC 延迟 | 0.5-2ms (网络+序列化) | 0.01-0.05ms (内存) |
| 1000 QPS CPU 占用 | 高 (网络栈) | 低 (直接调) |
| 序列化开销 | 必须 (protobuf) | 可选 (PyObject) |

业务方高 QPS 场景 (10k+ RPS) pyo3 有 5-10x 优势, 低 QPS (<100) 几乎无差。

### 3.2 部署复杂度

| 维度 | gRPC | pyo3 |
|---|---|---|
| 业务方需要 Rust toolchain | ❌ 不需要 (用 stub) | ✅ **需要** (build wheel) |
| 业务方需要 mah server | ✅ 必须 | ❌ 不需要 (in-process) |
| Python 版本绑定 | ❌ 任意 (grpcio wheel 跨版本) | ⚠️ **强绑定** (cpython 3.9/3.10/3.11/3.12 各自 wheel) |
| OS 绑定 | ❌ 任意 | ⚠️ **linux-x86_64 / macOS-aarch64 / win-amd64** 各自 |
| wheel 大小 | grpcio 5MB | ma-harness 30MB+ (含 .so) |
| pip install 时间 | 10s | 30-60s (含编译 / 下载大 wheel) |

**关键点**: 业务方需要 Rust toolchain 是大门槛。CI 系统需要支持 maturin + Rust toolchain。

### 3.3 维护成本

| 维度 | gRPC | pyo3 |
|---|---|---|
| 代码量 | 低 (复用 tonic stub) | 中 (要写 PyO3 包装层, 2-3 星期) |
| 测试 | stub mock + 真 server | pytest + 偶尔 .so 重 build |
| 升级 protobuf | stub 重生成, 业务方 0 改动 | API 变化要重 build wheel, 业务方 0 改动 |
| 维护者要求 | Python + Rust 异步 | Python + Rust 异步 + PyO3 内部 (gil, ref counting) |
| CI 复杂度 | cargo test + python -m pytest | + maturin build 多平台 + 测每个 cp 版本 |

### 3.4 业务方友好度

| 维度 | gRPC | pyo3 |
|---|---|---|
| 上手时间 | 30 分钟 (装 stub, 写 client) | 5 分钟 (`import ma_harness` 即可) |
| 单测 setup | 启动 server / mock | 直接调, 0 server 依赖 |
| 调试 | grpc logs + tcpdump | 跟普通 Python 库一样 |
| 类型提示 | stub gen 出来 (弱类型) | 强类型 (pyo3 自动转换) |

## 4. 决策矩阵

| 维度 | gRPC | pyo3 A (full in-process) | pyo3 B (embedded gRPC) | pyo3 C (hybrid) |
|---|---|---|---|---|
| 性能 | 中 | 高 | 中 | 高 (fallback 中) |
| 上手时间 | 30 min | 5 min | 5 min | 5 min |
| Rust toolchain 强制 | ❌ | ✅ | ✅ | ✅ |
| 单测 setup 复杂度 | 高 | 低 | 中 | 低 |
| 维护成本 | 低 | 中 | 中 | 高 |
| 跨 Python 版本 | 自由 | 锁 cp 3.9-3.12 | 锁 | 锁 |
| 大 wheel 依赖 | ❌ | ✅ (30MB+) | ✅ | ✅ |

### 4.1 推荐: **暂缓 pyo3, 等 gRPC binding 跑 3-6 月看业务反馈**

**理由**:
1. **gRPC 路径已完整**: 业务方能跑, 4 RPC demo + 4 streaming demo
2. **业务方需求未验证**: 没数据证明 pyo3 是真需求 (QPS / 单测 / 部署)
3. **Rust toolchain 门槛高**: 业务方 Python 团队不一定能装 Rust
4. **Phase 6 优先项更紧**: OpenaiAdapter SSE 解析 / AnthropicAdapter SSE / 真实 LLM 集成

**触发重新评估的条件**:
- 业务方反馈 gRPC 性能是瓶颈 (高 QPS 场景)
- 业务方反馈单测 setup 复杂 (mock server 难写)
- 业务方愿意接受 maturin build pipeline

### 4.2 如果要做 (Phase 7+)

推荐 **走法 C (hybrid)**, 但只在以下条件满足:
1. 业务方有 **2 个以上** 真实 Python 项目
2. 业务方有 **专用 Rust 工程师** 维护 native binding
3. 业务方有 **CI 能跑 maturin** (cross-platform wheel build)

实施路径:
- 新 crate `ma-harness-py` (cdylib)
- PyO3 包装 `ma-harness-core` (AgentLoop + EventLog + ModelAdapter)
- maturin 跨平台 build wheel
- PyPI publish (`pip install ma-harness`)
- 内部仍 gRPC 走 fallback (兼容性)

## 5. 给后来人

- **不要急着上 pyo3**: 走 gRPC binding 90% 业务方够用
- **真要上**: 优先 hybrid (走法 C), 业务方按需选
- **Rust 工具链**: 公司内是否有 Rust team 决定可行性
- **wheel build**: maturin 是当前最稳的选择, 比 setuptools-rust 简单
- **ABI 兼容**: 业务方 Python 版本必须跟 wheel cp 版本匹配 (cp39, cp310, ...)
- **CI 时间**: maturin build + 测试, 每个 PR 多 2-5 分钟
- **替代方案**: 如果只是想要 no-network, 可以走 embedded gRPC (走法 B) 业务方 0 改动

## 6. 参考

- [pyo3 官方 guide](https://pyo3.rs/v0.21.0/)
- [maturin 跨平台 build](https://www.maturin.rs/distribution)
- [Rust Python binding 选型 (2024)](https://kobzol.github.io/rust-python-interop/)
- 国内项目案例: 
  - [Polars (Rust dataframe + Python)](https://github.com/pola-rs/polars) — maturin 跨平台 wheel
  - [Pydantic v2 (Rust core)](https://github.com/pydantic/pydantic-core) — 完整 native binding
  - [Django ORM 5.0 (Rust 部分)](https://github.com/django/django) — 增量
