# ma-harness 多语言 binding (Phase 3.10 / T3.10 + P4-6/7)

ma-harness 提供 gRPC API (`crates/ma-harness-proto` 已定义 `.proto`),
任何支持 gRPC 的语言都能调。本目录给业务方提供 Python + Node.js (JS+TS) + Go 起点。

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

### Node.js + TypeScript 端 (P4-7 / Day 88)

```bash
cd bindings/node
npm install                              # + typescript + @types/node
npm run example:ts                       # tsc + node dist/example_client.js
```

跟 JS 版同样的 4 RPC 演示, 强类型:
- `tsc` 编译期 catch 字段名拼错 / 类型错
- 跟现代 Node.js backend 风格一致 (NestJS / Express + TS 业务方)
- 仍走 `@grpc/proto-loader` 运行时解析 (最小依赖, 业务方想完全类型
  可换 `ts-proto` 预编译)

`tsconfig.json` 跟 `example_client.ts` 走严格模式 (strict: true).
业务方可以 `import` 类型 / `extends` 现有 helper 函数, 直接接入.

## Go 端 (P4-6 / Day 87)

```bash
cd bindings/go
go mod tidy                              # 拉 grpc + protobuf
./compile_proto.sh                       # 编译 .proto → ma_harness_pb/
go run example_client.go                 # 跑 demo
```

Go 端用 `protoc-gen-go` + `protoc-gen-go-grpc` 走标准 protoc 工具链。
也支持 `buf generate` (`buf.gen.yaml` 已配), 业务方用 buf 更现代。

`example_client.go` 演示:
1. `grpc.Dial` + `WithBlock` + `WithTimeout` (启动连不上 fail fast)
2. `ListSessions` 列表
3. `CreateSession` 创一个新
4. `Run` 跑一次 agent (stub model, 不发真 LLM)
5. `GetSessionEvents` 拿事件
6. `defer conn.Close()` + `context.WithTimeout` 优雅退出

## Streaming RPC 演示 (P5-7 / Day 96)

`mah` 的 `AgentService.RunStream` RPC (proto 已定义) 返 server-streaming response。
业务方用 stub 拿 `Iterator` / `EventEmitter` / `ServerStream`,每个事件是 `AgentStreamEvent { run_id, message: Message }`。

### Python 端 (stream_client.py)

```bash
cd bindings/python
python stream_client.py
```

走 gRPC Python stub 走 `Iterator[AgentStreamEvent]`:
```python
stream = stub.RunStream(request)
for event in stream:
    token = event.message.content[0].text
    print(f"[token] {token!r}")
```

StubModelAdapter 把 user message 拆成 word 依次 yield ("alpha beta gamma" → 3 event)。

### Node.js 端 (stream_client.js)

```bash
cd bindings/node
node stream_client.js
```

走 `@grpc/grpc-js` 返 readable stream:
```js
const call = client.RunStream({...});
call.on('data', (event) => { /* event.message.content[0].text */ });
call.on('end', () => { /* done */ });
call.on('error', (err) => { /* err */ });
```

### Go 端 (stream_client.go)

```bash
cd bindings/go
go run stream_client.go
```

走 `server-stream` 走 `stream.Recv()` + `io.EOF`:
```go
stream, _ := stub.RunStream(ctx, req)
for {
    event, err := stream.Recv()
    if err == io.EOF { break }
    // event.Event 是 oneof, switch 类型拿 token
}
```

## 业务方集成

1. 把 `proto/` 目录 copy 到自己项目(版本锁)
2. 用对应语言的 protoc 工具生成 stub (Python `grpc_tools.protoc` / Node `@grpc/proto-loader` / Go `protoc-gen-go-grpc` / Java `protoc-gen-grpc-java`)
3. 用生成的 stub 调 gRPC server
4. (可选) 写自己的 client 包装 SDK 喂业务方用

## 其它语言示例

### Java

```xml
<dependency>
  <groupId>io.grpc</groupId>
  <artifactId>grpc-stub</artifactId>
</dependency>
```

用 `protobuf-maven-plugin` 编译 .proto → Java class.

## 限制 (Phase 3.10 + P4-6 + P5-7 PoC)

- 业务方需自己写 binding (我们只给 Python/Node/Go 起点 + .proto 契约)
- 不支持 server-streaming RPC wrapper (Python 直接用 stub `Iter` / Node `EventEmitter` / Go channel 即可)
- 不发 pyo3 / napi-rs 这种 native binding (Phase 4 看业务反馈)
- proto 版本手动 lock (Phase 4 加 semver check)

## 后续 (Phase 4)

- ~~加 Go binding example (高频语言)~~ — P4-6 完成 (Day 87)
- ~~加 TS-proto / d.ts for Node binding~~ — P4-7 完成 (Day 88, 走 tsc + proto-loader 兼容)
- ~~加 streaming RPC 演示~~ — P5-7 完成 (Day 96, Python/Node/Go stream_client.*)
- 加 OpenAPI → grpc-web 桥 (业务方浏览器直接调)
- pyo3 评估: 业务方拿 Python extension 不用走 gRPC 网络 (in-process)
