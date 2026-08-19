"""ma-harness Python streaming gRPC client example (P5-7 / Day 96).

跑:
    1. pip install -r requirements.txt
    2. python compile_proto.py
    3. mah start --grpc-port 50051 --http-port 50050
    4. python stream_client.py

演示:
- 调 AgentService.RunStream RPC
- 走 `for event in stub: yield event` 自动 iter (gRPC Python stub 走 Iterator)
- 每个 event 是 AgentStreamEvent { run_id, message: Message }
- 拿 message.content[0].text 拼起来

跟 example_client.py 区别:
- 同步 Run 走 stub.Run(req).message_response.content (单 string)
- 异步 RunStream 走 stub.RunStream(req) 拿 iterator, 每 token 一个 event
"""
import sys
from pathlib import Path

# 让 import 找 ma_harness_pb2 (跟 stream_client.py 同目录)
sys.path.insert(0, str(Path(__file__).resolve().parent))

import grpc  # noqa: E402

try:
    from ma_harness_pb2 import (
        agent_pb2, agent_pb2_grpc,
        ContentBlock, TextBlock,
        Message, ToolRole,
        AgentRunRequest, ModelConfig, ModelAdapter,
    )
except ImportError:
    print("ERROR: ma_harness_pb2 not found.")
    print("Run `python compile_proto.py` first to generate Python stubs.")
    sys.exit(1)


def run_stream(stub: agent_pb2_grpc.AgentServiceStub, session_id: str, message: str):
    """RunStream RPC 拿 token stream (P5-7 新增)

    Python gRPC stub 走 Iterator[AgentStreamEvent]
    业务方在 for 循环里拼 token, 跟 sync iterator 一样
    """
    request = AgentRunRequest(
        session_id=session_id,
        input=Message(
            role=ToolRole.TOOL_ROLE_USER,
            content=[ContentBlock(text=TextBlock(text=message))],
        ),
        model_config=ModelConfig(
            adapter=ModelAdapter.MODEL_ADAPTER_STUB,
            model="stub",
            temperature=0.0,
            max_tokens=100,
        ),
    )
    # RunStream 返 server-streaming response, Python gRPC 自动转 iterator
    return stub.RunStream(request)


def main() -> int:
    # 1. 连 gRPC server
    channel = grpc.insecure_channel("localhost:50051")
    stub = agent_pb2_grpc.AgentServiceStub(channel)

    # 2. 调 RunStream, iter 每个 AgentStreamEvent
    print("=== RunStream (3-word message) ===")
    stream = run_stream(stub, "stream-demo", "alpha beta gamma")

    collected = []
    for event in stream:
        # 每个 event: run_id + message (oneof)
        if event.HasField("message"):
            msg = event.message
            if msg.content and msg.content[0].HasField("text"):
                token = msg.content[0].text
                collected.append(token)
                # 实时打印 (没 flush, 业务方看 streaming effect)
                print(f"  [token] {token!r}", flush=True)

    full = "".join(collected)
    print(f"\n=== Done ===")
    print(f"  total events: {len(collected)}")
    print(f"  full content: {full!r}")
    # StubModelAdapter 把 user message "alpha beta gamma" 拆成 3 word yield
    # 3 word events 拼回 "alpha beta gamma "
    assert len(collected) == 3, f"expected 3 events, got {len(collected)}"
    assert full == "alpha beta gamma ", f"expected 'alpha beta gamma ', got {full!r}"
    print("OK: 3 word events 拼回 'alpha beta gamma '")

    channel.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
