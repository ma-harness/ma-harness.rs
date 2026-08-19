// 2026-08-19 (Day 101 / P7-1.1): gRPC-web 桥 类型定义
// 跟 crates/ma-harness-proto/proto/ma_harness/v1/*.proto 对齐
// 业务方改 proto 后, 同步改这里 + grpc.ts 跟 server tonic-web 桥

export interface ProtoSession {
  id: string;
  name: string;
  state: SessionState;
  mode: OperatingMode;
  created_at: string;  // ISO 8601
  updated_at: string;
  closed_at: string | null;
  enabled_plugins: string[];
  user_id: string;
  metadata: Record<string, string>;
  stats: SessionStats | null;
}

export enum SessionState {
  Unspecified = 0,
  Created = 1,
  Active = 2,
  Paused = 3,
  Closed = 4,
  Errored = 5,
  Cancelled = 6,
}

export enum OperatingMode {
  Default = 0,
  Ptc = 1,
  Minimal = 2,
  Creator = 3,
}

export interface SessionStats {
  total_events: number;
  total_tokens: number;
  total_prompt_tokens: number;
  total_completion_tokens: number;
  duration_ms: number;
}

export interface ProtoEvent {
  seq: number;
  session_id: string;
  event_type: string;
  severity: 'Info' | 'Warn' | 'Error' | 'Debug';
  payload_json: string;
  ts: string;
}

export interface ProtoRun {
  run_id: string;
  session_id: string;
  status: 'Running' | 'Completed' | 'Failed' | 'Cancelled';
  model: string;
  started_at: string;
  completed_at: string | null;
  output: string;
  error: string | null;
}

export interface CreateSessionRequest {
  name: string;
  enabled_plugins: string[];
  metadata: Record<string, string>;
}

export interface ListSessionsRequest {
  page_size: number;
  page_token: string;
}
