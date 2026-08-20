//! P11-4: ACP (Agent Communication Protocol) 集成 smoke test
//!
//! 跟 dsh jsonrpc-agent 风格一致: JSON-RPC 2.0 over stdio
//!
//! 跑: `cargo test --package ma-harness-cli --test acp_smoke -- --nocapture`
//! 需要 `mah` binary 已经 build 好 (cargo build 之后)

use std::io::{Read, Write};
use std::process::{Command, Stdio};

fn find_mah_exe() -> std::path::PathBuf {
    // 1. CARGO_BIN_EXE_<name> (cargo 提供的)
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_mah") {
        return std::path::PathBuf::from(path);
    }
    // 2. MAH_PATH env
    if let Ok(p) = std::env::var("MAH_PATH") {
        return std::path::PathBuf::from(p);
    }
    // 3. default cargo target dir
    let target = std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| "D:/rust_target".to_string());
    let exe = if cfg!(windows) { "mah.exe" } else { "mah" };
    let p = std::path::Path::new(&target).join("debug").join(exe);
    if p.exists() {
        return p;
    }
    panic!("mah binary not found. Build it with `cargo build --bin mah` or set MAH_PATH");
}

fn run_acp(input: &str) -> (String, String, i32) {
    let mah = find_mah_exe();
    let mut child = Command::new(&mah)
        .arg("acp")
        .arg("serve")
        .arg("--model")
        .arg("stub")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mah acp serve");

    let mut stdin = child.stdin.take().expect("stdin");
    stdin.write_all(input.as_bytes()).expect("write stdin");
    drop(stdin); // close stdin to signal EOF

    let output = child.wait_with_output().expect("wait output");
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (stdout, stderr, output.status.code().unwrap_or(-1))
}

/// P12-6 v2: 业务方跑交互式 ACP session (state 跨多次 call 共享)
///
/// 业务方用 Popen 持续 process, 业务方发多 message, server 状态保留 (e.g. newSession → loadSession)
fn run_acp_interactive<F>(script: F) -> (String, i32)
where
    F: FnOnce(&mut ChildProcess) + Send + 'static,
{
    let mah = find_mah_exe();
    let mut child = Command::new(&mah)
        .arg("acp")
        .arg("serve")
        .arg("--model")
        .arg("stub")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn mah acp serve");

    let mut proc = ChildProcess {
        stdin: child.stdin.take().expect("stdin"),
        stdout: child.stdout.take().expect("stdout"),
        stderr: child.stderr.take().expect("stderr"),
    };
    script(&mut proc);
    // 关闭 stdin, server 看到 EOF 退出
    drop(proc.stdin);

    let mut stdout_collected = String::new();
    proc.stdout
        .read_to_string(&mut stdout_collected)
        .expect("read stdout");
    let _ = proc.stderr; // 暂不收集 stderr

    let status = child.wait().expect("wait");
    (stdout_collected, status.code().unwrap_or(-1))
}

struct ChildProcess {
    stdin: std::process::ChildStdin,
    stdout: std::process::ChildStdout,
    stderr: std::process::ChildStderr,
}

impl ChildProcess {
    fn write_line(&mut self, line: &str) {
        self.stdin.write_all(line.as_bytes()).expect("write");
        self.stdin.write_all(b"\n").expect("write newline");
        self.stdin.flush().expect("flush");
    }
    fn read_line(&mut self) -> Option<String> {
        let mut buf = [0u8; 1];
        let mut line = Vec::new();
        loop {
            match self.stdout.read(&mut buf) {
                Ok(0) => return None, // EOF
                Ok(_) => {
                    if buf[0] == b'\n' {
                        return Some(String::from_utf8_lossy(&line).to_string());
                    }
                    line.push(buf[0]);
                }
                Err(e) => panic!("read err: {e}"),
            }
        }
    }
}

#[test]
fn acp_initialize() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}
"#;
    let (stdout, _stderr, code) = run_acp(input);
    assert_eq!(code, 0, "expected clean exit");
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(resp["jsonrpc"], "2.0");
    assert_eq!(resp["id"], 1);
    assert_eq!(resp["result"]["protocolVersion"], 1);
    assert_eq!(resp["result"]["agentInfo"]["name"], "ma-harness");
}

#[test]
fn acp_new_session() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"newSession","params":{"cwd":"/tmp","mcpServers":[]}}
"#;
    let (stdout, _stderr, code) = run_acp(input);
    assert_eq!(code, 0);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(resp["id"], 1);
    let session_id = resp["result"]["sessionId"]
        .as_str()
        .expect("sessionId string");
    assert!(!session_id.is_empty());
    // UUID format check (len >= 32)
    assert!(session_id.len() >= 32, "session_id too short: {session_id}");
}

#[test]
fn acp_prompt_full_lifecycle() {
    // 1. initialize, 2. newSession, 3. prompt (应收到 1 notification + 1 response)
    // 用新生成的 UUID 作为 sessionId (跟 newSession 响应一致)
    let session_id = uuid::Uuid::new_v4().to_string();
    let input = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":1,"clientCapabilities":{{}}}}}}
{{"jsonrpc":"2.0","id":2,"method":"newSession","params":{{"cwd":"/tmp","mcpServers":[]}}}}
{{"jsonrpc":"2.0","id":3,"method":"prompt","params":{{"sessionId":"{session_id}","prompt":[{{"type":"text","text":"hello world"}}]}}}}
"#
    );
    let (stdout, _stderr, code) = run_acp(&input);
    assert_eq!(code, 0);

    // stdout 应有 4 行: 3 responses (init/newSession/prompt) + 1 notification (session/update)
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(
        lines.len(),
        4,
        "expected 4 lines, got {}: {:?}",
        lines.len(),
        lines
    );

    // 1. initialize response
    let r1: serde_json::Value = serde_json::from_str(lines[0]).expect("parse line 0");
    assert_eq!(r1["id"], 1);
    assert_eq!(r1["result"]["protocolVersion"], 1);

    // 2. newSession response
    let r2: serde_json::Value = serde_json::from_str(lines[1]).expect("parse line 1");
    assert_eq!(r2["id"], 2);
    let new_session_id = r2["result"]["sessionId"].as_str().expect("sessionId");

    // 3. session/update notification (text chunk) — sessionId 来自 prompt 请求
    let n3: serde_json::Value = serde_json::from_str(lines[2]).expect("parse line 2");
    assert_eq!(n3["method"], "session/update");
    assert_eq!(n3["params"]["sessionId"], session_id);
    assert_eq!(
        n3["params"]["update"]["sessionUpdate"],
        "agent_message_chunk"
    );
    let text = n3["params"]["update"]["content"]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.contains("hello world") || text.contains("echo"),
        "expected text: {text}"
    );

    // 4. prompt response
    let r4: serde_json::Value = serde_json::from_str(lines[3]).expect("parse line 3");
    assert_eq!(r4["id"], 3);
    assert_eq!(r4["result"]["stopReason"], "end_turn");

    // sanity: newSession 返回的 sessionId 跟 prompt 用的 sessionId 不一样 (v1 server 独立分配)
    assert_ne!(
        new_session_id, session_id,
        "v1 server 新建 session 跟 prompt sessionId 独立"
    );
}

#[test]
fn acp_unknown_method_returns_error() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"not_a_method","params":{}}
"#;
    let (stdout, _stderr, code) = run_acp(input);
    assert_eq!(code, 0);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(resp["id"], 1);
    assert!(
        resp["error"].is_object(),
        "expected error object, got: {resp}"
    );
    assert_eq!(resp["error"]["code"], -32601); // Method not found
}

#[test]
fn acp_invalid_json_returns_parse_error() {
    let input = "this is not valid json\n";
    let (stdout, _stderr, code) = run_acp(input);
    assert_eq!(code, 0);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert!(resp["error"].is_object(), "expected error object");
    assert_eq!(resp["error"]["code"], -32700); // Parse error
}

// === P12-6 v2: loadSession / cancel / image content tests ===

#[test]
fn acp_initialize_v2_capabilities() {
    // P12-6 v2: loadSession=true, image=true
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}
"#;
    let (stdout, _stderr, _code) = run_acp(input);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert_eq!(resp["result"]["agentCapabilities"]["loadSession"], true);
    assert_eq!(
        resp["result"]["agentCapabilities"]["promptCapabilities"]["image"],
        true
    );
}

#[test]
fn acp_load_session_returns_metadata() {
    // P12-6 v2: 用 interactive runner (业务方 state 共享)
    let (stdout, _code) = run_acp_interactive(|proc| {
        // 1. newSession
        proc.write_line(
            r#"{"jsonrpc":"2.0","id":1,"method":"newSession","params":{"cwd":"/tmp/test"}}"#,
        );
        let r1_line = proc.read_line().expect("read r1");
        let r1: serde_json::Value = serde_json::from_str(&r1_line).expect("parse r1");
        let actual_id = r1["result"]["sessionId"]
            .as_str()
            .expect("sessionId from newSession")
            .to_string();

        // 2. loadSession with actual id
        let load_req = format!(
            r#"{{"jsonrpc":"2.0","id":2,"method":"loadSession","params":{{"sessionId":"{actual_id}"}}}}"#
        );
        proc.write_line(&load_req);
        let r2_line = proc.read_line().expect("read r2");
        let r2: serde_json::Value = serde_json::from_str(&r2_line).expect("parse r2");
        assert_eq!(r2["id"], 2);
        assert_eq!(r2["result"]["sessionId"], actual_id);
        assert_eq!(r2["result"]["cwd"], "/tmp/test");
        assert_eq!(r2["result"]["messageCount"], 0);
    });
    // stdout 包含 r1 (但 read_line 内部已消费)
    let _ = stdout;
}

#[test]
fn acp_load_session_not_found() {
    let input = r#"{"jsonrpc":"2.0","id":1,"method":"loadSession","params":{"sessionId":"00000000-0000-0000-0000-000000000000"}}
"#;
    let (stdout, _stderr, _code) = run_acp(input);
    let resp: serde_json::Value = serde_json::from_str(stdout.trim()).expect("parse JSON");
    assert!(resp["error"].is_object());
    assert_eq!(resp["error"]["code"], -32603);
}

#[test]
fn acp_cancel_returns_cancelled_flag() {
    // 1. newSession, 2. cancel
    let new_id = uuid::Uuid::new_v4().to_string();
    let input = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"newSession","params":{{}}}}
{{"jsonrpc":"2.0","id":2,"method":"cancel","params":{{"sessionId":"{new_id}"}}}}
"#
    );
    let (stdout, _stderr, _code) = run_acp(&input);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2);
    let r2: serde_json::Value = serde_json::from_str(lines[1]).expect("parse line 1");
    assert_eq!(r2["id"], 2);
    assert_eq!(r2["result"]["cancelled"], true);
    assert_eq!(r2["result"]["sessionId"], new_id);
}

#[test]
fn acp_prompt_with_image_block() {
    // P12-6 v2: prompt 接受 text + image content blocks
    let new_id = uuid::Uuid::new_v4().to_string();
    let input = format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"newSession","params":{{}}}}
{{"jsonrpc":"2.0","id":2,"method":"prompt","params":{{"sessionId":"{new_id}","prompt":[{{"type":"text","text":"describe"}},{{"type":"image","source":{{"type":"base64","media_type":"image/png","data":"iVBORw0KGgo="}}}}]}}}}
"#
    );
    let (stdout, _stderr, _code) = run_acp(&input);
    let lines: Vec<&str> = stdout.lines().filter(|l| !l.trim().is_empty()).collect();
    // 期望 3 行: newSession response + session/update notif + prompt response
    assert!(lines.len() >= 3, "got {} lines: {:?}", lines.len(), lines);
    let r3: serde_json::Value = serde_json::from_str(lines[2]).expect("parse line 2");
    assert_eq!(r3["id"], 2);
    // stub 返 [+1 image(s)] 标记
    let r_notif: serde_json::Value = serde_json::from_str(lines[1]).expect("parse line 1");
    assert_eq!(r_notif["method"], "session/update");
    let text = r_notif["params"]["update"]["content"]["text"]
        .as_str()
        .expect("text");
    assert!(
        text.contains("+1 image"),
        "expected 'image' marker in: {text}"
    );
}
