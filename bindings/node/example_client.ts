// ma-harness TypeScript gRPC client example.
//
// 跑:
//   1. npm install
//   2. npx tsc                         # 编译 .ts → dist/ (类型检查 + 转换)
//   3. node dist/example_client.js      # 跑编译后的 JS
//   或者:
//   1. npm install
//   2. npm run example:ts              # 一键 tsc + node
//
// 跟 example_client.js 同样的 4 RPC 演示, 区别是 TypeScript:
//   - 强类型 proto message 字段 (IntelliSense)
//   - 编译期 catch 字段名拼错 / 类型错
//   - 跟现代 Node.js backend 风格一致
//
// 备注: 仍然走 @grpc/proto-loader 运行时解析 .proto (不预编译 stub),
//       类型用 grpc.loadPackageDefinition 走 any (跟 JS 一样), 业务方
//       可以用 ts-proto 替换拿完全类型, 这里演示最小依赖.

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
const SessionClient = proto.ma_harness.v1.SessionService;

const ADDR = 'localhost:50051';

function listSessions(client: any): Promise<any[]> {
  return new Promise((resolve, reject) => {
    client.ListSessions({ page: 1, page_size: 10 }, (err: any, resp: any) => {
      if (err) return reject(err);
      resolve(resp.sessions || []);
    });
  });
}

function createSession(client: any, name: string): Promise<string> {
  return new Promise((resolve, reject) => {
    client.CreateSession(
      { name, operating_mode: 'OPERATING_MODE_DEFAULT' },
      (err: any, resp: any) => {
        if (err) return reject(err);
        resolve(resp.session.id);
      }
    );
  });
}

function runAgent(client: any, sessionId: string, message: string): Promise<string> {
  return new Promise((resolve, reject) => {
    client.Run(
      {
        session_id: sessionId,
        user_message: message,
        model: 'stub',
        temperature: 0.7,
        max_tokens: 1024,
      },
      (err: any, resp: any) => {
        if (err) return reject(err);
        resolve(resp.model_response.content);
      }
    );
  });
}

function getSessionEvents(client: any, sessionId: string): Promise<any[]> {
  return new Promise((resolve, reject) => {
    client.GetSessionEvents(
      { session_id: sessionId, limit: 20 },
      (err: any, resp: any) => {
        if (err) return reject(err);
        resolve(resp.events || []);
      }
    );
  });
}

async function main(): Promise<void> {
  const sessionClient = new SessionClient(ADDR, grpc.credentials.createInsecure());
  const agentClient = new AgentClient(ADDR, grpc.credentials.createInsecure());

  console.log('=== Existing sessions ===');
  const sessions = await listSessions(sessionClient);
  for (const s of sessions) {
    console.log(`  ${s.id.slice(0, 8)}... name=${JSON.stringify(s.name)} state=${s.state}`);
  }

  console.log('\n=== Creating new session ===');
  const newId: string = await createSession(sessionClient, 'ts-example');
  console.log(`  new session id = ${newId}`);

  console.log('\n=== Running agent (stub model) ===');
  const content: string = await runAgent(agentClient, newId, 'hello from TypeScript');
  console.log(`  response: ${JSON.stringify(content)}`);

  console.log('\n=== Session events ===');
  const events = await getSessionEvents(sessionClient, newId);
  for (const e of events.slice(0, 5)) {
    console.log(`  seq=${e.seq} type=${e.event_type} severity=${e.severity}`);
  }
}

main().catch((err) => {
  console.error('ERROR:', err);
  process.exit(1);
});
