"""Example 02: 用真 LLM (需要 OPENAI_API_KEY / ANTHROPIC_API_KEY).

业务方 `export OPENAI_API_KEY=sk-...` 或在代码里 setenv.
"""
import os

from mah_py import Mah, MahError


def main():
    # 必须设 API key
    if "OPENAI_API_KEY" not in os.environ and "ANTHROPIC_API_KEY" not in os.environ:
        print("Set OPENAI_API_KEY or ANTHROPIC_API_KEY before running this example")
        return

    # OpenAI gpt-4o-mini (便宜)
    if "OPENAI_API_KEY" in os.environ:
        model = "openai:gpt-4o-mini"
    else:
        model = "anthropic:claude-3-5-sonnet-latest"

    try:
        with Mah(model=model) as m:
            r = m.run("Explain quantum entanglement in 50 words.")
            print(f"model: {model}")
            print(f"content: {r.content}")
            print(f"tokens:  prompt={r.prompt_tokens} + completion={r.completion_tokens}")
    except MahError as e:
        print(f"failed: {e}")


if __name__ == "__main__":
    main()
