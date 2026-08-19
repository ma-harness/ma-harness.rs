#!/usr/bin/env python3
"""Convert dsh acp-snapshot fixtures to ma-harness dsh_format JSONL.

dsh acp-snapshot fixture structure (per folder under tests/fixtures/):
- input.json: { "steps": [{"op": "initialize"}, {"op": "newSession"}, {"op": "prompt", "text": "..."}] }
- stdout.expected.jsonl: JSON-RPC 2.0 expected messages from agent
- behavior.json: { "prompt": "respond", "logs": [...] } or similar
- session.jsonl: agent session events (request/header, assistant/chunk, etc.)
- session.{N}.jsonl: child session events
- system-prompt.{N}.expected.md: expected system prompt
- tool-schemas.{N}.expected.json: expected tool schemas

Mapping to ma-harness dsh_format (one fixture per dsh folder):
- name: folder name (e.g. "dsh_snap_plain_turn")
- category: "agent_run" (most dsh snapshots are agent runs)
- input.session_id: "dsh-snap-{folder}"
- input.messages: derived from input.steps (initialize + newSession → system; prompt → user)
- input.events: from session.jsonl (request/header → ModelRequest, etc.)
- expected_output.events: derived from stdout.expected.jsonl (ModelResponse events)
- expected_output.messages: empty (dsh doesn't have assistant messages in expected)
"""
import json
import sys
from pathlib import Path

DSH_FIXTURE_ROOT = Path(r"D:\workspace\learn\deepseek-ai\deepseek-harness\packages\test-support\acp-snapshot\tests\fixtures")
OUTPUT_JSONL = Path(r"D:\tmp\dsh_snap_converted.jsonl")


def dsh_event_type_to_ma(dsh_type: str) -> str:
    """Map dsh session event type to ma-harness event type."""
    mapping = {
        "session": "SessionStart",
        "request/header": "ModelRequest",
        "assistant/chunk": "ModelResponse",
        "assistant/turn_end": "RunEnd",
        "turn/start": "RunStart",
        "turn/end": "RunEnd",
        "user/message": "UserInput",
        "user/prompt": "UserInput",
        "hook/result": "ApprovalDecision",
        "hook_result": "ApprovalDecision",
        "tool/call": "ToolCall",
        "tool/result": "ToolResult",
        "tool/error": "ToolError",
    }
    return mapping.get(dsh_type, dsh_type.replace("/", "_"))


def parse_session_jsonl(path: Path) -> list[dict]:
    """Read session.jsonl → list of dsh events."""
    if not path.exists():
        return []
    events = []
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            events.append(json.loads(line))
    return events


def parse_stdout_expected(path: Path) -> list[dict]:
    """Read stdout.expected.jsonl → list of JSON-RPC 2.0 messages."""
    if not path.exists():
        return []
    msgs = []
    with path.open() as f:
        for line in f:
            line = line.strip()
            if not line:
                continue
            msgs.append(json.loads(line))
    return msgs


def session_events_to_ma_input(events: list[dict]) -> list[dict]:
    """Convert dsh session events to ma-harness FixtureEvent format.

    Returns list of { "type": "...", "payload": {...} }.
    """
    out = []
    for e in events:
        et = e.get("type", "")
        ma_type = dsh_event_type_to_ma(et)
        if et == "session":
            payload = {
                "id": e.get("id", ""),
                "cwd": e.get("cwd", ""),
                "delegation_depth": e.get("delegationDepth", 0),
            }
            if "parentSession" in e:
                payload["parent_session"] = e["parentSession"]
        elif et == "request/header":
            header = e.get("data", {}).get("header", {})
            payload = {
                "model": header.get("config", {}).get("model", ""),
                "reason": header.get("reason", ""),
            }
        elif et == "assistant/chunk":
            chunk = e.get("data", {}).get("chunk", {})
            payload = {
                "content": chunk.get("text", ""),
                "turn": e.get("data", {}).get("turn", 1),
                "step": e.get("data", {}).get("step", 1),
            }
        else:
            payload = e.get("data", {})
        out.append({"type": ma_type, "payload": payload})
    return out


def stdout_expected_to_ma_expected(msgs: list[dict]) -> list[dict]:
    """Convert JSON-RPC 2.0 expected messages to ma-harness ExpectedEvent format."""
    out = []
    for m in msgs:
        # Response to initialize
        if "result" in m and m.get("result", {}).get("protocolVersion") is not None:
            out.append({
                "type": "ProtocolHandshake",
                "payload_match": {"protocolVersion": m["result"]["protocolVersion"]},
            })
        # Response to newSession
        elif "result" in m and m.get("result", {}).get("sessionId"):
            sid = m["result"]["sessionId"]
            if sid != "{{sessionId}}":
                out.append({
                    "type": "SessionStart",
                    "payload_match": {"sessionId": sid},
                })
        # session/update notifications
        elif m.get("method") == "session/update":
            update = m.get("params", {}).get("update", {})
            update_type = update.get("sessionUpdate", "")
            content = update.get("content", {})
            text = content.get("text", "") if content.get("type") == "text" else ""
            if update_type == "agent_message_chunk":
                out.append({
                    "type": "ModelResponse",
                    "payload_match": {"content": text},
                })
            elif update_type == "tool_call":
                out.append({
                    "type": "ToolCall",
                    "payload_match": {"tool": content.get("name", "")},
                })
            else:
                out.append({
                    "type": update_type,
                    "payload_match": {"data": content},
                })
        # Response to prompt (stopReason)
        elif "result" in m and m.get("result", {}).get("stopReason"):
            out.append({
                "type": "RunEnd",
                "payload_match": {"stopReason": m["result"]["stopReason"]},
            })
    return out


def input_json_to_messages(input_json: dict) -> list[dict]:
    """Convert input.json steps to dsh messages array."""
    msgs = []
    for step in input_json.get("steps", []):
        op = step.get("op", "")
        if op == "prompt":
            msgs.append({"role": "user", "content": step.get("text", "")})
    return msgs


def convert_fixture(folder: Path) -> dict:
    """Convert one dsh fixture folder to one ma-harness dsh_format JSONL entry.

    dsh snapshot test: input = session.jsonl (agent internal log), expected = stdout.expected.jsonl.
    But our conformance replays input.events and compares to expected. So we put session.jsonl
    as BOTH input and expected (replay identity check), with stdout.expected as auxiliary metadata
    in the description (for traceability).
    """
    name = folder.name
    input_json_path = folder / "input.json"
    input_data = json.loads(input_json_path.read_text()) if input_json_path.exists() else {"steps": []}

    # Read all session.jsonl (parent + children numbered)
    all_session_events = []
    for path in sorted(folder.glob("session*.jsonl")):
        all_session_events.extend(parse_session_jsonl(path))

    # Read stdout expected (kept for description metadata, not used in compare)
    stdout_msgs = parse_stdout_expected(folder / "stdout.expected.jsonl")

    # Convert: same events for input and expected (replay identity)
    messages = input_json_to_messages(input_data)
    ma_events = session_events_to_ma_input(all_session_events)

    # Build expected events in dsh DshEvent format (type + data) — empty data object
    expected_events = []
    for e in ma_events:
        expected_events.append({
            "type": e["type"],
            "data": {},
        })

    description_parts = [f"dsh acp-snapshot {name}"]
    description_parts.append(f"({len(all_session_events)} session events")
    if stdout_msgs:
        description_parts.append(f", {len(stdout_msgs)} JSON-RPC expected)")
    else:
        description_parts.append(")")
    description = " ".join(description_parts)

    return {
        "name": f"dsh_snap_{name.replace('-', '_')}",
        "category": "agent_run",
        "description": description,
        "input": {
            "session_id": f"dsh-snap-{name}",
            "events": ma_events,
            "messages": messages,
        },
        "expected_output": {
            "events": expected_events,
            "messages": [],
        },
    }


def main():
    OUTPUT_JSONL.parent.mkdir(parents=True, exist_ok=True)
    converted = []
    for sub in ["suite", "record-suite"]:
        sub_dir = DSH_FIXTURE_ROOT / sub
        if not sub_dir.exists():
            continue
        for folder in sorted(sub_dir.iterdir()):
            if not folder.is_dir():
                continue
            converted.append(convert_fixture(folder))
    with OUTPUT_JSONL.open("w") as f:
        for c in converted:
            f.write(json.dumps(c) + "\n")
    print(f"Converted {len(converted)} dsh snapshot fixtures → {OUTPUT_JSONL}")
    for c in converted:
        n_in = len(c["input"]["events"])
        n_msg = len(c["input"]["messages"])
        n_out = len(c["expected_output"]["events"])
        print(f"  {c['name']:35s} in[ev={n_in} msg={n_msg}] out[ev={n_out}]")


if __name__ == "__main__":
    main()
