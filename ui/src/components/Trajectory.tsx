import { useState, useMemo } from 'react';
import type { ProtoEvent } from '@/api/types';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P7-1.4): Trajectory 视图完整版
// - 时间线: System / User / Assistant / Tool / Error / Context
// - 折叠 payload (点击展开 JSON)
// - 顶部搜索 (按 event_type + payload 文本)
// - 简化版: 跟 dsh Trajectory 类似 (P7-5 会做多列 + 持久化筛选)

const SEVERITY_COLORS = {
  Info: 'text-fg-accent',
  Warn: 'text-warn',
  Error: 'text-error',
  Debug: 'text-fg-muted',
} as const;

const EVENT_TYPE_ICONS: Record<string, string> = {
  SessionStart: '▶',
  SessionEnd: '■',
  RunStart: '↗',
  RunEnd: '↘',
  ModelRequest: '↑',
  ModelResponse: '↓',
  ToolCall: '⚙',
  ToolResult: '✓',
  ToolError: '✗',
  UserInput: '✎',
  ApprovalRequest: '🔒',
  ApprovalDecision: '🔓',
};

export default function Trajectory({ events }: { events: ProtoEvent[] }) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState('');

  // 过滤 events (按 event_type + payload 文本)
  const filtered = useMemo(() => {
    if (!query.trim()) return events;
    const q = query.toLowerCase();
    return events.filter((e) => {
      if (e.event_type.toLowerCase().includes(q)) return true;
      if (e.payload_json && e.payload_json.toLowerCase().includes(q)) return true;
      return false;
    });
  }, [events, query]);

  if (events.length === 0) {
    return <div className="text-fg-muted">No events yet. Run the agent to see the trajectory.</div>;
  }

  const toggleExpanded = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) {
        next.delete(key);
      } else {
        next.add(key);
      }
      return next;
    });
  };

  return (
    <div className="flex h-full flex-col">
      {/* Search bar */}
      <div className="mb-2 flex items-center gap-2">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="🔍 filter by event type or payload…"
          className="flex-1 rounded border border-border bg-bg px-3 py-1 text-xs text-fg placeholder:text-fg-muted focus:border-border-accent"
        />
        <span className="text-xs text-fg-muted">
          {filtered.length} / {events.length}
        </span>
      </div>

      {/* Event list */}
      <div className="flex-1 space-y-1 overflow-auto font-mono text-xs">
        {filtered.map((e, i) => {
          const key = `${e.seq}-${i}`;
          const isExpanded = expanded.has(key);
          const icon = EVENT_TYPE_ICONS[e.event_type] ?? '·';
          const colorClass = SEVERITY_COLORS[e.severity] ?? 'text-fg-muted';
          return (
            <div key={key} className="rounded border border-transparent hover:border-border hover:bg-bg-panel">
              {/* Summary row (clickable) */}
              <div
                onClick={() => toggleExpanded(key)}
                className="grid cursor-pointer grid-cols-[auto_8rem_8rem_1fr] items-start gap-3 px-2 py-1"
              >
                <span className={cn('select-none text-center', colorClass)}>{icon}</span>
                <span className={cn('truncate', colorClass)}>{e.event_type}</span>
                <span className="text-fg-muted">{new Date(e.ts).toLocaleTimeString()}</span>
                <span className="truncate text-fg-muted">
                  {summarizePayload(e.payload_json)}
                </span>
              </div>
              {/* Expanded payload */}
              {isExpanded && (
                <div className="border-t border-border bg-bg px-4 py-2">
                  <pre className="overflow-x-auto whitespace-pre-wrap text-fg-muted">
                    {prettyJson(e.payload_json)}
                  </pre>
                  {e.error_message && (
                    <div className="mt-2 text-error">⚠ {e.error_message}</div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function summarizePayload(json: string): string {
  try {
    const obj = JSON.parse(json);
    if (typeof obj === 'string') return obj;
    if (obj.content) return String(obj.content).slice(0, 100);
    if (obj.tool) return `${obj.tool}(${JSON.stringify(obj.args || {}).slice(0, 50)})`;
    if (obj.tool_name) return `${obj.tool_name} (${obj.risk_level || 'unknown risk'})`;
    if (obj.result) return String(obj.result).slice(0, 100);
    if (obj.error) return `ERR: ${obj.error}`;
    return JSON.stringify(obj).slice(0, 80);
  } catch {
    return json.slice(0, 80);
  }
}

function prettyJson(json: string): string {
  try {
    return JSON.stringify(JSON.parse(json), null, 2);
  } catch {
    return json;
  }
}
