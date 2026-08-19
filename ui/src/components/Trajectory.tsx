import { useState, useMemo } from 'react';
import type { ProtoEvent } from '@/api/types';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P10-4): Trajectory 视图多列布局
// - 4 列: time / type / severity / payload preview (跟 dsh Trajectory 类似)
// - 折叠 payload (点击展开 pretty JSON)
// - 顶部 filter (按 type + 文本)
// - 持久化筛选: localStorage 保存 filter query, 业务方刷新页面还在
// - multi-select 类型过滤 (chips)

const SEVERITY_COLORS: Record<string, string> = {
  Info: 'text-fg-accent',
  Warn: 'text-warn',
  Error: 'text-error',
  Debug: 'text-fg-muted',
};

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

const STORAGE_KEY_FILTER = 'mah_trajectory_filter';
const STORAGE_KEY_TYPES = 'mah_trajectory_types';

export default function Trajectory({ events }: { events: ProtoEvent[] }) {
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState(() => localStorage.getItem(STORAGE_KEY_FILTER) || '');
  const [enabledTypes, setEnabledTypes] = useState<Set<string>>(() => {
    const saved = localStorage.getItem(STORAGE_KEY_TYPES);
    return saved ? new Set(JSON.parse(saved)) : new Set();
  });

  // 持久化: query
  const updateQuery = (q: string) => {
    setQuery(q);
    localStorage.setItem(STORAGE_KEY_FILTER, q);
  };

  // 持久化: enabledTypes
  const toggleType = (t: string) => {
    const next = new Set(enabledTypes);
    if (next.has(t)) {
      next.delete(t);
    } else {
      next.add(t);
    }
    setEnabledTypes(next);
    localStorage.setItem(STORAGE_KEY_TYPES, JSON.stringify(Array.from(next)));
  };

  // 多列过滤: 文本 + 类型 chips
  const filtered = useMemo(() => {
    return events.filter((e) => {
      if (enabledTypes.size > 0 && !enabledTypes.has(e.event_type)) {
        return false;
      }
      if (query.trim()) {
        const q = query.toLowerCase();
        if (
          !e.event_type.toLowerCase().includes(q) &&
          !(e.payload_json || '').toLowerCase().includes(q)
        ) {
          return false;
        }
      }
      return true;
    });
  }, [events, query, enabledTypes]);

  // 出现过的 event types (chips)
  const allTypes = useMemo(() => {
    const set = new Set<string>();
    events.forEach((e) => set.add(e.event_type));
    return Array.from(set).sort();
  }, [events]);

  if (events.length === 0) {
    return <div className="text-fg-muted">No events yet. Run the agent to see the trajectory.</div>;
  }

  const toggleExpanded = (key: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  };

  return (
    <div className="flex h-full flex-col gap-2">
      {/* Filter bar: text + type chips */}
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={query}
            onChange={(e) => updateQuery(e.target.value)}
            placeholder="🔍 filter by event type or payload…"
            className="flex-1 rounded border border-border bg-bg px-3 py-1 text-xs text-fg placeholder:text-fg-muted focus:border-border-accent"
          />
          <span className="text-xs text-fg-muted whitespace-nowrap">
            {filtered.length} / {events.length}
          </span>
          {enabledTypes.size > 0 && (
            <button
              onClick={() => {
                setEnabledTypes(new Set());
                localStorage.removeItem(STORAGE_KEY_TYPES);
              }}
              className="text-xs text-fg-muted hover:text-fg"
            >
              clear ({enabledTypes.size})
            </button>
          )}
        </div>
        {/* Type chips (multi-select toggle) */}
        {allTypes.length > 0 && (
          <div className="flex flex-wrap gap-1">
            {allTypes.map((t) => {
              const active = enabledTypes.size === 0 || enabledTypes.has(t);
              const isFiltering = enabledTypes.has(t);
              return (
                <button
                  key={t}
                  onClick={() => toggleType(t)}
                  className={cn(
                    'rounded-full border px-2 py-0.5 text-[10px]',
                    isFiltering
                      ? 'border-border-accent bg-border-accent text-white'
                      : active
                        ? 'border-border text-fg-muted hover:border-fg-muted'
                        : 'border-border text-fg-muted/40 hover:text-fg-muted',
                  )}
                >
                  {EVENT_TYPE_ICONS[t] ?? '·'} {t}
                </button>
              );
            })}
          </div>
        )}
      </div>

      {/* Event list: 多列 grid (time | icon | type | severity | payload) */}
      <div className="flex-1 space-y-0.5 overflow-auto font-mono text-xs">
        {filtered.map((e, i) => {
          const key = `${e.seq}-${i}`;
          const isExpanded = expanded.has(key);
          const icon = EVENT_TYPE_ICONS[e.event_type] ?? '·';
          const typeColor = SEVERITY_COLORS[e.severity] ?? 'text-fg-muted';
          return (
            <div
              key={key}
              className="rounded border border-transparent hover:border-border hover:bg-bg-panel"
            >
              <div
                onClick={() => toggleExpanded(key)}
                className="grid cursor-pointer grid-cols-[5rem_1.5rem_8rem_5rem_1fr] items-start gap-2 px-2 py-1"
              >
                <span className="text-fg-muted text-[10px]">
                  {new Date(e.ts).toLocaleTimeString()}
                </span>
                <span className={cn('select-none text-center', typeColor)}>{icon}</span>
                <span className={cn('truncate', typeColor)}>{e.event_type}</span>
                <span className={cn('text-[10px]', typeColor)}>{e.severity.toLowerCase()}</span>
                <span className="truncate text-fg-muted">{summarizePayload(e.payload_json)}</span>
              </div>
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
