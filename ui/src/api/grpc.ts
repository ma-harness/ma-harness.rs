// 2026-08-19 (Day 101 / P7-1.1): gRPC-web client wrapper (placeholder)
// 完整实现在 P7-1.2 (tonic-web 集成 + Vite proxy 配置)
//
// 当前是 stub — 业务方现在 import 用不会工作, P7-1.2 之后会真接 gRPC-web.
// 业务方开发时可以 mock 数据先写 UI, 接到真 gRPC 时改 fetchSessionList 即可.

import type {
  CreateSessionRequest,
  ListSessionsRequest,
  ProtoEvent,
  ProtoRun,
  ProtoSession,
} from './types';

const API_BASE = '/api';  // Vite dev server proxy → tonic :50050

export async function fetchSessionList(req: ListSessionsRequest): Promise<{
  sessions: ProtoSession[];
  next_page_token: string;
}> {
  // TODO(P7-1.2): 替换为 gRPC-web request, 现在是 mock data
  const r = await fetch(`${API_BASE}/v1/sessions?page_size=${req.page_size}`);
  if (!r.ok) throw new Error(`fetchSessionList: HTTP ${r.status}`);
  const data = await r.json();
  return {
    sessions: data.sessions || [],
    next_page_token: data.next_page_token || '',
  };
}

export async function fetchSession(id: string): Promise<ProtoSession> {
  const r = await fetch(`${API_BASE}/v1/sessions/${encodeURIComponent(id)}`);
  if (!r.ok) throw new Error(`fetchSession: HTTP ${r.status}`);
  return r.json();
}

export async function createSession(req: CreateSessionRequest): Promise<ProtoSession> {
  const r = await fetch(`${API_BASE}/v1/sessions`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });
  if (!r.ok) throw new Error(`createSession: HTTP ${r.status}`);
  return r.json();
}

export async function closeSession(id: string): Promise<void> {
  const r = await fetch(`${API_BASE}/v1/sessions/${encodeURIComponent(id)}/close`, {
    method: 'POST',
  });
  if (!r.ok) throw new Error(`closeSession: HTTP ${r.status}`);
}

export async function fetchSessionEvents(id: string, sinceSeq = 0): Promise<ProtoEvent[]> {
  const r = await fetch(
    `${API_BASE}/v1/sessions/${encodeURIComponent(id)}/events?since_seq=${sinceSeq}`,
  );
  if (!r.ok) throw new Error(`fetchSessionEvents: HTTP ${r.status}`);
  const data = await r.json();
  return data.events || [];
}

export async function runAgent(req: {
  session_id: string;
  input: string;
  model: string;
  enabled_plugins: string[];
}): Promise<ProtoRun> {
  const r = await fetch(`${API_BASE}/v1/runs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  });
  if (!r.ok) throw new Error(`runAgent: HTTP ${r.status}`);
  return r.json();
}

// 实时事件流 (P7-1.7): gRPC RunStream → EventSource SSE
export function streamSessionEvents(
  id: string,
  onEvent: (e: ProtoEvent) => void,
  onError: (err: Error) => void,
): () => void {
  const url = `${API_BASE}/v1/sessions/${encodeURIComponent(id)}/events/stream`;
  const es = new EventSource(url);
  es.onmessage = (ev) => {
    try {
      onEvent(JSON.parse(ev.data) as ProtoEvent);
    } catch (e) {
      onError(e instanceof Error ? e : new Error(String(e)));
    }
  };
  es.onerror = () => onError(new Error('EventSource error'));
  return () => es.close();
}
