"""Example 05: 错误处理 (mah binary 找不到 / 跑失败 / timeout)."""
import os
from pathlib import Path

from mah_py import Mah, MahError


def case_1_no_mah():
    """MAH_PATH 指向不存在路径 → MahError"""
    try:
        Mah(mah_path="/nonexistent/mah").version()
    except MahError as e:
        print(f"case 1 (bad path) caught: {e}")


def case_2_garbage_path():
    """PATH 里没 mah → MahError"""
    saved_path = os.environ.pop("PATH", None)
    saved_mah = os.environ.pop("MAH_PATH", None)
    try:
        Mah().version()
    except MahError as e:
        print(f"case 2 (no mah in PATH) caught: {e}")
    finally:
        if saved_path is not None:
            os.environ["PATH"] = saved_path
        if saved_mah is not None:
            os.environ["MAH_PATH"] = saved_mah


def case_3_timeout():
    """短 timeout + 长跑任务 → MahError"""
    try:
        with Mah(timeout=0.001) as m:  # 1ms timeout, 必 timeout
            m.run("do something slow")
    except MahError as e:
        print(f"case 3 (timeout) caught: {e}")


def case_4_mah_failure():
    """传无效 model 让 mah 失败 → MahError (非-zero exit code)"""
    # mah CLI 对未注册 model 会走 stub fallback 不一定 fail, 这里只示意 API
    try:
        with Mah() as m:
            r = m.run("test", model="nonexistent:no-such-model-xyz")
            print(f"case 4 (no error, fell back to stub): {r.content[:50]}")
    except MahError as e:
        print(f"case 4 (mah failed) caught: {e}")


def main():
    print("=== mah-py error handling examples ===")
    case_1_no_mah()
    case_2_garbage_path()
    case_3_timeout()
    case_4_mah_failure()
    print("=== done ===")


if __name__ == "__main__":
    main()
