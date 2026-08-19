import { useNavigate, useParams } from 'react-router-dom';
import { useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchSession, fetchSessionEvents, streamSessionEvents } from '@/api/grpc';
import { useSessionStore } from '@/store/sessionStore';
import { ArrowLeft, Play, Square } from 'lucide-react';
import { useState } from 'react';
import type { ProtoEvent } from '@/api/types';
import Trajectory from '@/components/Trajectory';

// 2026-08-19 (Day 101 / P7-1.3 + 1.4 + 1.7):
// Session Detail view — 头部 (id / state / name) + 中间 (Run 控件) + 底部 (Trajectory 时间线)
// P7-1.7: 实时事件流通过 streamSessionEvents push 到 store
// P7-1.4: Trajectory 组件渲染 System/User/Assistant/Tool/Error 时间线 (本路由占位, P7-1.5 完整版)

export default function SessionDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [input, setInput] = useState('');
  const { events, appendEvents, clearEvents } = useSessionStore();

  // 拿 session 详情
  const { data: session, isLoading } = useQuery({
    queryKey: ['session', id],
    queryFn: () => fetchSession(id!),
    enabled: !!id,
  });

  // 拿历史 events
  const { data: historical } = useQuery({
    queryKey: ['events', id],
    queryFn: () => fetchSessionEvents(id!),
    enabled: !!id,
  });

  // 历史 events 进 store (P7-1.7 的 stream append 后面接上)
  useEffect(() => {
    if (historical) {
      clearEvents();
      appendEvents(historical);
    }
  }, [historical, appendEvents, clearEvents]);

  // 实时事件流
  useEffect(() => {
    if (!id) return;
    const cleanup = streamSessionEvents(
      id,
      (e) => appendEvents([e]),
      (err) => console.warn('stream error:', err),
    );
    return cleanup;
  }, [id, appendEvents]);

  if (isLoading) return <div className="p-6 text-fg-muted">Loading…</div>;
  if (!session) return <div className="p-6 text-error">Session not found</div>;

  return (
    <div className="flex h-full flex-col">
      {/* Header */}
      <div className="flex items-center gap-4 border-b border-border bg-bg-panel px-6 py-3">
        <button
          onClick={() => navigate('/sessions')}
          className="flex items-center gap-1 text-fg-muted hover:text-fg"
        >
          <ArrowLeft className="h-4 w-4" />
          Back
        </button>
        <div className="flex-1">
          <div className="font-mono text-sm">{session.id}</div>
          <div className="text-xs text-fg-muted">{session.name || '(unnamed)'}</div>
        </div>
        <span className="text-xs text-fg-muted">
          {events.length} events
        </span>
      </div>

      {/* Run input + button (P7-1.7 完整 RunStream UI) */}
      <div className="flex items-center gap-2 border-b border-border bg-bg-panel px-6 py-3">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="input message…"
          className="flex-1 rounded border border-border bg-bg px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-border-accent"
          onKeyDown={(e) => {
            if (e.key === 'Enter' && input.trim()) {
              // TODO(P7-1.7): 调 runAgent + stream events
              console.log('Run:', input);
            }
          }}
        />
        <button
          disabled={!input.trim()}
          className="flex items-center gap-1 rounded border border-border-accent bg-border-accent px-3 py-1.5 text-sm text-white hover:opacity-90 disabled:opacity-50"
        >
          <Play className="h-3 w-3" />
          Run
        </button>
        <button className="flex items-center gap-1 rounded border border-border bg-bg-panel px-3 py-1.5 text-sm hover:bg-bg-hover">
          <Square className="h-3 w-3" />
          Cancel
        </button>
      </div>

      {/* Trajectory (System/User/Assistant/Tool/Context 时间线) */}
      <div className="flex-1 overflow-auto p-6">
        <Trajectory events={events} />
      </div>
    </div>
  );
}
