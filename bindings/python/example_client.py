"""ma-harness Python gRPC client example.

跑:
    1. pip install -r requirements.txt
    2. python compile_proto.py
    3. mah start --grpc-port 50051 --http-port 50050
    4. python example_client.py

演示:
- ListSessions: 列出所有 session
- CreateSession: 创建一个新 session
- RunAgent: 跑一次 agent (本地 stub model, 不真 LLM)
- GetSessionEvents: 拿 session 事件
"""
import sys
from pathlib import Path

# 让 import 找 ma_harness_pb2 (跟 example_client.py 同目录)
sys.path.insert(0, str(Path(__file__).resolve().parent))

import grpc  # noqa: E402

# 业务方实际:  from ma_harness_pb2 import agent_pb2, session_pb2, agent_pb2_grpc, session_pb2_grpc
# 这里演示 placeholder (proto 没真编译时 import 会失败, 业务方先跑 compile_proto.py)
try:
    from ma_harness_pb2 import agent_pb2, agent_pb2_grpc, session_pb2, session_pb2_grpc
except ImportError:
    print("ERROR: ma_harness_pb2 not found.")
    print("Run `python compile_proto.py` first to generate Python stubs.")
    sys.exit(1)


def list_sessions(stub: session_pb2_grpc.SessionServiceStub) -> list:
    """ListSessions RPC call"""
    request = session_pb2.ListSessionsRequest(limit=10)
    response = stub.ListSessions(request)
    return list(response.sessions)


def create_session(stub: session_pb2_grpc.SessionServiceStub, name: str = "python-client") -> str:
    """CreateSession RPC call → 返回新 session id"""
    request = session_pb2.CreateSessionRequest(
        name=name,
        operating_mode=session_pb2.OPERATING_MODE_DEFAULT,
    )
    response = stub.CreateSession(request)
    return response.session.id


def run_agent(stub: agent_pb2_grpc.AgentServiceStub, session_id: str, message: str) -> str:
    """Run RPC call → 返回 model response content"""
    request = agent_pb2.RunRequest(
        session_id=session_id,
        user_message=message,
        model="stub",
        temperature=0.7,
        max_tokens=1024,
    )
    response = stub.Run(request)
    return response.model_response.content


def get_session_events(stub: session_pb2_grpc.SessionServiceStub, session_id: str) -> list:
    """GetSessionEvents RPC call"""
    request = session_pb2.GetSessionEventsRequest(
        session_id=session_id,
        limit=20,
    )
    response = stub.GetSessionEvents(request)
    return list(response.events)


def main() -> int:
    # 1. 连 gRPC server
    channel = grpc.insecure_channel("localhost:50051")
    agent_stub = agent_pb2_grpc.AgentServiceStub(channel)
    session_stub = session_pb2_grpc.SessionServiceStub(channel)

    # 2. 列出现有 session
    print("=== Existing sessions ===")
    sessions = list_sessions(session_stub)
    for s in sessions:
        print(f"  {s.id[:8]}... name={s.name!r} state={s.state}")

    # 3. 创建一个新 session
    print("\n=== Creating new session ===")
    new_id = create_session(session_stub, name="python-example")
    print(f"  new session id = {new_id}")

    # 4. 跑一次 agent (stub model, 不发真 LLM)
    print("\n=== Running agent (stub model) ===")
    content = run_agent(agent_stub, new_id, "hello from Python")
    print(f"  response: {content!r}")

    # 5. 拿 events
    print("\n=== Session events ===")
    events = get_session_events(session_stub, new_id)
    for e in events[:5]:
        print(f"  seq={e.seq} type={e.event_type} severity={e.severity}")

    channel.close()
    return 0


if __name__ == "__main__":
    sys.exit(main())
