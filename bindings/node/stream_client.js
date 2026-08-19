// ma-harness Node.js streaming gRPC client example (P5-7 / Day 96).
//
// 跑:
//   1. npm install
//   2. node stream_client.js
//
// 演示:
// - 调 AgentService.RunStream RPC
// - 走 gRPC server-streaming, Node 端用 `stub.RunStream(req)` 拿 EventEmitter
// - 监听 'data' 事件拿 AgentStreamEvent, 'end' 拿结束
// - 'error' 拿 error
//
// 跟 example_client.js 区别:
// - 同步 Run 走 stub.Run(req, callback) (单 string)
// - 异步 RunStream 走 stub.RunStream(req) 返 readable stream

const path = require('path');
const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const PROTO_DIR = path.join(REPO_ROOT, 'proto');

const packageDef = protoLoader.loadSync(
  [
    path.join(PROTO_DIR, 'ma_harness/v1/agent.proto'),
    path.join(PROTO_DIR, 'ma_harness/v1/session.proto'),
    path.join(PROTO_DIR, 'ma_harness/v1/event.proto'),
  ],
  {
    keepCase: true,
    longs: String,
    enums: String,
    defaults: true,
    oneofs: true,
    includeDirs: [PROTO_DIR],
  }
);

const proto = grpc.loadPackageDefinition(packageDef).ma_harness.v1;
const AgentClient = proto.AgentService;

const ADDR = 'localhost:50051';

function runStream(client, sessionId, message) {
  return new Promise((resolve, reject) => {
    const call = client.RunStream({
      session_id: sessionId,
      input: {
        role: 'TOOL_ROLE_USER',
        content: [{ text: message }],
      },
      model_config: {
        adapter: 'MODEL_ADAPTER_STUB',
        model: 'stub',
        temperature: 0,
        max_tokens: 100,
      },
    });

    const tokens = [];
    call.on('data', (event) => {
      // event.message.content[0].text
      const text = event.message?.content?.[0]?.text;
      if (text) {
        tokens.push(text);
        process.stdout.write(`  [token] ${JSON.stringify(text)}\n`);
      }
    });
    call.on('end', () => resolve(tokens));
    call.on('error', reject);
  });
}

async function main() {
  const client = new AgentClient(ADDR, grpc.credentials.createInsecure());

  console.log('=== RunStream (3-word message) ===');
  const tokens = await runStream(client, 'stream-demo', 'alpha beta gamma');

  const full = tokens.join('');
  console.log(`\n=== Done ===`);
  console.log(`  total events: ${tokens.length}`);
  console.log(`  full content: ${JSON.stringify(full)}`);
  if (tokens.length !== 3) {
    console.error(`FAIL: expected 3 events, got ${tokens.length}`);
    process.exit(1);
  }
  if (full !== 'alpha beta gamma ') {
    console.error(`FAIL: expected 'alpha beta gamma ', got ${JSON.stringify(full)}`);
    process.exit(1);
  }
  console.log("OK: 3 word events 拼回 'alpha beta gamma '");
}

main().catch((err) => {
  console.error('ERROR:', err);
  process.exit(1);
});
