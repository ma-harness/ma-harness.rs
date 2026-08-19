"""编译 ma-harness .proto → Python stub.

跑:
    python compile_proto.py

会:
1. 调 grpc_tools.protoc 编 proto/ma_harness/v1/{agent,session,event}.proto
2. 输出到 ma_harness_pb2/ 子目录
3. 业务方 from ma_harness_pb2 import agent_pb2, session_pb2, event_pb2
"""
import os
import subprocess
import sys
from pathlib import Path

# 仓库根 (此脚本在 bindings/python/)
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
PROTO_DIR = REPO_ROOT / "proto"
OUT_DIR = Path(__file__).resolve().parent / "ma_harness_pb2"


def main() -> int:
    OUT_DIR.mkdir(exist_ok=True)
    # grpc_tools.protoc 是 grpc 自带的 protoc 包装 (不需要本地 protoc.exe)
    cmd = [
        sys.executable, "-m", "grpc_tools.protoc",
        f"-I{PROTO_DIR}",
        f"--python_out={OUT_DIR}",
        f"--grpc_python_out={OUT_DIR}",
        # ma-harness 用 google/protobuf/timestamp.proto (标准 well-known)
        str(PROTO_DIR / "ma_harness" / "v1" / "agent.proto"),
        str(PROTO_DIR / "ma_harness" / "v1" / "session.proto"),
        str(PROTO_DIR / "ma_harness" / "v1" / "event.proto"),
    ]
    print(f"Running: {' '.join(cmd)}")
    result = subprocess.run(cmd, capture_output=True, text=True)
    if result.returncode != 0:
        print(f"protoc failed:\nstdout: {result.stdout}\nstderr: {result.stderr}")
        return 1
    print(f"OK: {OUT_DIR}/agent_pb2.py + session_pb2.py + event_pb2.py + *_pb2_grpc.py")
    print(f"  含 server / client stub, 业务方直接 import 即可")
    return 0


if __name__ == "__main__":
    sys.exit(main())
