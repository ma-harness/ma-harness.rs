"""Example 03: 多轮对话 (固定 session ID).

业务方用同一个 session_id 调多次 run, 上下文保留.
"""
from mah_py import Mah, MahError


def main():
    session_id = "demo-session-001"
    try:
        with Mah() as m:
            r1 = m.run("My name is Alice", session=session_id)
            print(f"turn 1: {r1.content}")

            r2 = m.run("What's my name?", session=session_id)
            print(f"turn 2: {r2.content}")
            # 期望: agent 记得 "Alice"
    except MahError as e:
        print(f"failed: {e}")


if __name__ == "__main__":
    main()
