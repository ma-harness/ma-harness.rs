// ma-harness TypeScript streaming gRPC client example (P5-7 / Day 96).
//
// 跑:
//   1. npm install
//   2. npx tsc
//   3. node dist/stream_client.js
//   或者: npm run example:stream  (假设 scripts 里有这一行, 业务方按需加)
//
// 跟 stream_client.js 同样的 4 步:
//   1. 调 RunStream RPC
//   2. listen 'data' event
//   3. 拼 token
//   4. assert 3 word event
//
// TypeScript 跟 JS 区别: 强类型 callback (Promise wrapper 走 async/await)

import * as path from 'path';
import * as grpc from '@grpc/grpc-js';
import * as protoLoader from '@grpc/proto-loader';

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

const proto = grpc.loadPackageDefinition(packageDef) as any;
const AgentClient = proto.ma_harness.v1.AgentService;

const ADDR = 'localhost:50051';

/** Promise 包装: 'data' 收 token, 'end' resolve, 'error' reject */
function runStreamAsync(
  client: any,
  sessionId: string,
  message: string,
): Promise<string[]> {
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

    const tokens: string[] = [];
    call.on('data', (event: any) => {
      const text = event.message?.content?.[0]?.text;
      if (text) {
        tokens.push(text);
        process.stdout.write(`  [token] ${JSON.stringify(text)}\n`);
      }
    });
    call.on('end', () => resolve(tokens));
    call.on('error', (err: any) => reject(err));
  });
}

async function main(): Promise<void> {
  const client = new AgentClient(ADDR, grpc.credentials.createInsecure());

  console.log('=== RunStream (3-word message) ===');
  const tokens: string[] = await runStreamAsync(client, 'stream-demo', 'alpha beta gamma');

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
