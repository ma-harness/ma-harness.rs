"""Example 01: hello world.

最小 ma-harness 跑法 (默认 stub model, 不用 API key).
"""
from mah_py import Mah, MahError


def main():
    try:
        with Mah() as m:
            r = m.run("Say hi in 5 words.")
            print(f"session: {r.session_id}")
            print(f"run:     {r.run_id}")
            print(f"content: {r.content}")
            print(f"tokens:  prompt={r.prompt_tokens} completion={r.completion_tokens}")
    except MahError as e:
        print(f"failed: {e}")


if __name__ == "__main__":
    main()
