// ma-harness Node.js gRPC client example
// 跑:
//   1. npm install
//   2. node example_client.js
//
// 演示:
// - ListSessions: 列出所有 session
// - CreateSession: 创建一个新 session
// - RunAgent: 跑一次 agent (本地 stub model, 不真 LLM)
// - GetSessionEvents: 拿 session 事件

const path = require('path');
const grpc = require('@grpc/grpc-js');
const protoLoader = require('@grpc/proto-loader');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const PROTO_DIR = path.join(REPO_ROOT, 'proto');

// 加载所有 .proto (agent / session / event)
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
const SessionClient = proto.SessionService;

const ADDR = 'localhost:50051';

function listSessions(client) {
  return new Promise((resolve, reject) => {
    client.ListSessions({ limit: 10 }, (err, resp) => {
      if (err) return reject(err);
      resolve(resp.sessions || []);
    });
  });
}

function createSession(client, name) {
  return new Promise((resolve, reject) => {
    client.CreateSession(
      { name, operating_mode: 'OPERATING_MODE_DEFAULT' },
      (err, resp) => {
        if (err) return reject(err);
        resolve(resp.session.id);
      }
    );
  });
}

function runAgent(client, sessionId, message) {
  return new Promise((resolve, reject) => {
    client.Run(
      {
        session_id: sessionId,
        user_message: message,
        model: 'stub',
        temperature: 0.7,
        max_tokens: 1024,
      },
      (err, resp) => {
        if (err) return reject(err);
        resolve(resp.model_response.content);
      }
    );
  });
}

function getSessionEvents(client, sessionId) {
  return new Promise((resolve, reject) => {
    client.GetSessionEvents(
      { session_id: sessionId, limit: 20 },
      (err, resp) => {
        if (err) return reject(err);
        resolve(resp.events || []);
      }
    );
  });
}

async function main() {
  const sessionClient = new SessionClient(ADDR, grpc.credentials.createInsecure());
  const agentClient = new AgentClient(ADDR, grpc.credentials.createInsecure());

  console.log('=== Existing sessions ===');
  const sessions = await listSessions(sessionClient);
  for (const s of sessions) {
    console.log(`  ${s.id.slice(0, 8)}... name=${JSON.stringify(s.name)} state=${s.state}`);
  }

  console.log('\n=== Creating new session ===');
  const newId = await createSession(sessionClient, 'node-example');
  console.log(`  new session id = ${newId}`);

  console.log('\n=== Running agent (stub model) ===');
  const content = await runAgent(agentClient, newId, 'hello from Node');
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
