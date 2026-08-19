# ma-harness 多语言 binding (Phase 3.10 / T3.10)

ma-harness 提供 gRPC API (`crates/ma-harness-proto` 已定义 `.proto`),
任何支持 gRPC 的语言都能调。本目录给业务方提供 Python + Node.js 起点。

## 跟 Rust 端的契约

- **proto 定义**: `proto/ma_harness/v1/{agent,session,event}.proto`
- **gRPC endpoint**: `localhost:50051` (默认, `mah start --grpc-port`)
- **服务**:
  - `ma_harness.v1.AgentService` — `Run` / `StreamRun` RPC
  - `ma_harness.v1.SessionService` — `ListSessions` / `CreateSession` / `GetSession` / `GetSessionEvents` / `CloseSession` RPC

## Python 端

```bash
cd bindings/python
pip install -r requirements.txt         # grpcio + grpcio-tools
python compile_proto.py                  # 编译 .proto → ma_harness_pb2/
python example_client.py                 # 跑 demo (ListSessions + CreateSession + Run)
```

`example_client.py` 演示:
1. 拿 gRPC channel
2. `ListSessions` 列表
3. `CreateSession` 创一个新
4. `Run` 跑一次 agent (stub model, 不发真 LLM)
5. `GetSessionEvents` 拿事件

## Node.js 端

```bash
cd bindings/node
npm install                              # @grpc/grpc-js + @grpc/proto-loader
node example_client.js                    # 跑 demo
```

Node 端用 `@grpc/proto-loader` **运行时解析 .proto**,不用预编译。
代码里 `protoLoader.loadSync` + `grpc.loadPackageDefinition` 拿 stub.

## 业务方集成

1. 把 `proto/` 目录 copy 到自己项目(版本锁)
2. 用对应语言的 protoc 工具生成 stub (Python `grpc_tools.protoc` / Node `@grpc/proto-loader` / Go `protoc-gen-go-grpc` / Java `protoc-gen-grpc-java`)
3. 用生成的 stub 调 gRPC server
4. (可选) 写自己的 client 包装 SDK 喂业务方用

## 其它语言示例

### Go

```bash
go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
protoc -I proto --go_out=. --go-grpc_out=. proto/ma_harness/v1/*.proto
```

### Java

```xml
<dependency>
  <groupId>io.grpc</groupId>
  <artifactId>grpc-stub</artifactId>
</dependency>
```

用 `protobuf-maven-plugin` 编译 .proto → Java class.

## 限制 (Phase 3.10 PoC)

- 业务方需自己写 binding (我们只给 Python/Node 起点 + .proto 契约)
- 不支持 server-streaming RPC wrapper (Python/Node 直接用 stub 的 `Iter` / `EventEmitter` 即可)
- 不发 pyo3 / napi-rs 这种 native binding (Phase 4 看业务反馈)
- proto 版本手动 lock (Phase 4 加 semver check)

## 后续 (Phase 4)

- 加 Go binding example (高频语言)
- 加 OpenAPI → grpc-web 桥 (业务方浏览器直接调)
- 加 streaming RPC 演示 (Python `Iter`, Node `EventEmitter`)
- pyo3 评估: 业务方拿 Python extension 不用走 gRPC 网络 (in-process)
