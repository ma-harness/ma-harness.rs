#!/usr/bin/env bash
# 编译 ma-harness .proto → Go stub
#
# 跑:
#   ./compile_proto.sh
#
# 要:
# 1. protoc 装好 (apt: apt install -y protobuf-compiler, brew: brew install protobuf)
# 2. protoc-gen-go + protoc-gen-go-grpc 装好
#    go install google.golang.org/protobuf/cmd/protoc-gen-go@latest
#    go install google.golang.org/grpc/cmd/protoc-gen-go-grpc@latest
# 3. (推荐) 装 buf 一键:
#    go install github.com/bufbuild/buf/cmd/buf@latest
#    buf generate
#
# 输出: ma_harness_pb/ 子目录, 业务方 import "github.com/ma-harness/ma-harness-client/ma_harness_pb"

set -e

REPO_ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
PROTO_DIR="$REPO_ROOT/proto"
OUT_DIR="$(cd "$(dirname "$0")" && pwd)/ma_harness_pb"

mkdir -p "$OUT_DIR"

protoc \
  -I"$PROTO_DIR" \
  --go_out="$OUT_DIR" --go_opt=paths=source_relative \
  --go-grpc_out="$OUT_DIR" --go-grpc_opt=paths=source_relative \
  "$PROTO_DIR/ma_harness/v1/agent.proto" \
  "$PROTO_DIR/ma_harness/v1/session.proto" \
  "$PROTO_DIR/ma_harness/v1/event.proto"

echo "Generated Go stubs in $OUT_DIR"
ls -la "$OUT_DIR/ma_harness/v1/"
