import type { ProtoEvent } from '@/api/types';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P7-1.4): Trajectory 视图
// 时间线: System / User / Assistant / Tool / Error / Context
// 跟 dsh 的 Trajectory 类似 (按时间线展示, 左侧 type + 时间, 右侧 payload 摘要)
// 简化版 — P7-5 会做完整版 (着色 / 折叠 / 搜索 / 持久化筛选)

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
};

export default function Trajectory({ events }: { events: ProtoEvent[] }) {
  if (events.length === 0) {
    return <div className="text-fg-muted">No events yet. Run the agent to see the trajectory.</div>;
  }

  return (
    <div className="space-y-1 font-mono text-xs">
      {events.map((e, i) => {
        const icon = EVENT_TYPE_ICONS[e.event_type] ?? '·';
        const colorClass = SEVERITY_COLORS[e.severity];
        return (
          <div
            key={`${e.seq}-${i}`}
            className="grid grid-cols-[auto_8rem_8rem_1fr] items-start gap-3 rounded border border-transparent px-2 py-1 hover:border-border hover:bg-bg-panel"
          >
            {/* Icon */}
            <span className={cn('select-none text-center', colorClass)}>{icon}</span>
            {/* Event type */}
            <span className={cn('truncate', colorClass)}>{e.event_type}</span>
            {/* Timestamp */}
            <span className="text-fg-muted">{new Date(e.ts).toLocaleTimeString()}</span>
            {/* Payload summary */}
            <span className="truncate text-fg-muted">
              {summarizePayload(e.payload_json)}
            </span>
          </div>
        );
      })}
    </div>
  );
}

function summarizePayload(json: string): string {
  try {
    const obj = JSON.parse(json);
    if (typeof obj === 'string') return obj;
    if (obj.content) return String(obj.content).slice(0, 100);
    if (obj.tool) return `${obj.tool}(${JSON.stringify(obj.args || {}).slice(0, 50)})`;
    if (obj.result) return String(obj.result).slice(0, 100);
    if (obj.error) return `ERR: ${obj.error}`;
    return JSON.stringify(obj).slice(0, 80);
  } catch {
    return json.slice(0, 80);
  }
}
