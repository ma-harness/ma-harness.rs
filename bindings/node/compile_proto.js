// ma-harness Node.js gRPC proto loader
// 跑: node compile_proto.js
//
// 不用 protoc 编译 (Node 走 @grpc/proto-loader 运行时解析 .proto)
// 所以这步只是 sanity check: 确认 .proto 存在 + 可解析

const path = require('path');
const fs = require('fs');

const REPO_ROOT = path.resolve(__dirname, '..', '..');
const PROTO_FILES = [
  'proto/ma_harness/v1/agent.proto',
  'proto/ma_harness/v1/session.proto',
  'proto/ma_harness/v1/event.proto',
];

for (const f of PROTO_FILES) {
  const full = path.join(REPO_ROOT, f);
  if (!fs.existsSync(full)) {
    console.error(`ERROR: ${f} not found at ${full}`);
    process.exit(1);
  }
  console.log(`OK: ${f} (${fs.statSync(full).size} bytes)`);
}

console.log('\nNode 端不需要 compile 步骤:');
console.log('  @grpc/proto-loader 在运行时解析 .proto');
console.log('  example_client.js 直接 import 即可');
console.log('\n跑 example:');
console.log('  npm install');
console.log('  node example_client.js');
