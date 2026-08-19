"""Example 04: 跑 conformance fixture (跟 `mah conformance` 一致).

业务方跑 dsh_synthetic (7/7 期望 100%) 或 smoke (5/8 期望 62.5% 3 by design).
"""
from pathlib import Path

from mah_py import Mah, MahError


def main():
    # 找 fixture 路径 (跟 examples/ 同级 ../../crates/ma-harness-conformance/fixtures/)
    fixture_root = (
        Path(__file__).parent.parent.parent / "ma-harness-conformance" / "fixtures"
    )

    try:
        with Mah() as m:
            # dsh_synthetic: 7/7 期望 100% (P11-1.5 收官)
            dsh_path = fixture_root / "dsh_synthetic.jsonl"
            print(f"=== dsh_synthetic ({dsh_path.name}) ===")
            summary = m.conformance(fixtures=dsh_path, dsh=True)
            # 只打最后两行 (跳过 DEBUG)
            for line in summary.splitlines()[-3:]:
                print(line)

            # dsh-snap-converted: 9/9 期望 100% (P11-2 收官)
            dsh_snap = fixture_root / "dsh-snap-converted" / "dsh_snap.jsonl"
            if dsh_snap.exists():
                print(f"\n=== dsh-snap-converted ({dsh_snap.name}) ===")
                summary = m.conformance(fixtures=dsh_snap, dsh=True)
                for line in summary.splitlines()[-3:]:
                    print(line)
    except MahError as e:
        print(f"failed: {e}")


if __name__ == "__main__":
    main()
