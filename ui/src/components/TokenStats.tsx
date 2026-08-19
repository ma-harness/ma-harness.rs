import { useMemo } from 'react';
import type { ProtoEvent } from '@/api/types';

// 2026-08-19 (Day 101 / P7-1.5): Token 监控 widget
//
// 从 ModelRequest / ModelResponse events 累加 prompt_tokens + completion_tokens.
// 简化版: 不区分模型; P8-2 会加 per-model breakdown + 实时 SSE push.

interface TokenSummary {
  prompt: number;
  completion: number;
  total: number;
  requestCount: number;
  responseCount: number;
}

function summarize(events: ProtoEvent[]): TokenSummary {
  let prompt = 0;
  let completion = 0;
  let requestCount = 0;
  let responseCount = 0;
  for (const e of events) {
    if (e.event_type !== 'ModelRequest' && e.event_type !== 'ModelResponse') continue;
    let payload: Record<string, unknown> = {};
    try {
      payload = e.payload_json ? JSON.parse(e.payload_json) : {};
    } catch {
      continue;
    }
    if (e.event_type === 'ModelRequest') {
      requestCount += 1;
      // prompt_tokens 可能在 payload 或 derived from message length
      const pt = (payload.prompt_tokens as number) ?? (payload.estimated_tokens as number) ?? 0;
      prompt += pt;
    } else {
      responseCount += 1;
      const ct = (payload.completion_tokens as number) ?? 0;
      completion += ct;
    }
  }
  return { prompt, completion, total: prompt + completion, requestCount, responseCount };
}

export default function TokenStats({ events }: { events: ProtoEvent[] }) {
  const stats = useMemo(() => summarize(events), [events]);

  return (
    <div className="grid grid-cols-4 gap-3 border-b border-border bg-bg-panel px-6 py-2 text-xs">
      <Stat label="Requests" value={stats.requestCount} />
      <Stat label="Responses" value={stats.responseCount} />
      <Stat label="Prompt" value={stats.prompt} />
      <Stat label="Completion" value={stats.completion} />
    </div>
  );
}

function Stat({ label, value }: { label: string; value: number }) {
  return (
    <div className="flex flex-col">
      <span className="text-fg-muted">{label}</span>
      <span className="font-mono text-sm text-fg">{value.toLocaleString()}</span>
    </div>
  );
}
