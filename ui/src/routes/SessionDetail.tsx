import { useNavigate, useParams } from 'react-router-dom';
import { useEffect } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchSession, fetchSessionEvents, runAgent, streamSessionEvents } from '@/api/grpc';
import { useSessionStore } from '@/store/sessionStore';
import { ArrowLeft, Play, Square } from 'lucide-react';
import { useState } from 'react';
import Trajectory from '@/components/Trajectory';
import TokenStats from '@/components/TokenStats';

// 2026-08-19 (Day 101 / P7-1.3 + 1.4 + 1.5 + 1.7):
// Session Detail view — Header / Run 控件 / Token 监控 / Trajectory 时间线
// P7-1.7: streamSessionEvents 实时 push 到 store
// P7-1.4: Trajectory payload 折叠 + 搜索
// P7-1.5: TokenStats widget 累加 prompt/completion tokens

export default function SessionDetail() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const [input, setInput] = useState('');
  const { events, appendEvents, clearEvents } = useSessionStore();
  const queryClient = useQueryClient();

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

  // 历史 events 进 store
  useEffect(() => {
    if (historical) {
      clearEvents();
      appendEvents(historical);
    }
  }, [historical, appendEvents, clearEvents]);

  // 实时事件流 (P7-1.7)
  useEffect(() => {
    if (!id) return;
    const cleanup = streamSessionEvents(
      id,
      (e) => appendEvents([e]),
      (err) => console.warn('stream error:', err),
    );
    return cleanup;
  }, [id, appendEvents]);

  // Run mutation (P7-1.3: 真接 runAgent)
  const runMutation = useMutation({
    mutationFn: (msg: string) =>
      runAgent({
        session_id: id!,
        input: msg,
        model: 'stub',
        enabled_plugins: [],
      }),
    onSuccess: () => {
      // 后端会 push events, 不主动 refetch
      setInput('');
    },
    onError: (err) => {
      console.error('run error:', err);
    },
  });

  // 关闭 session
  const closeMutation = useMutation({
    mutationFn: () => import('@/api/grpc').then((m) => m.closeSession(id!)),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['sessions'] });
      navigate('/sessions');
    },
  });

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

      {/* Run input + button */}
      <div className="flex items-center gap-2 border-b border-border bg-bg-panel px-6 py-3">
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="input message…"
          disabled={runMutation.isPending}
          className="flex-1 rounded border border-border bg-bg px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-border-accent disabled:opacity-50"
          onKeyDown={(e) => {
            if (e.key === 'Enter' && input.trim() && !runMutation.isPending) {
              runMutation.mutate(input);
            }
          }}
        />
        <button
          onClick={() => input.trim() && runMutation.mutate(input)}
          disabled={!input.trim() || runMutation.isPending}
          className="flex items-center gap-1 rounded border border-border-accent bg-border-accent px-3 py-1.5 text-sm text-white hover:opacity-90 disabled:opacity-50"
        >
          <Play className="h-3 w-3" />
          {runMutation.isPending ? 'Running…' : 'Run'}
        </button>
        <button
          onClick={() => closeMutation.mutate()}
          disabled={closeMutation.isPending}
          className="flex items-center gap-1 rounded border border-border bg-bg-panel px-3 py-1.5 text-sm hover:bg-bg-hover disabled:opacity-50"
        >
          <Square className="h-3 w-3" />
          Close
        </button>
      </div>

      {/* P7-1.5: Token 监控 widget */}
      <TokenStats events={events} />

      {/* P7-1.4: Trajectory (折叠 + 搜索) */}
      <div className="flex-1 overflow-auto p-6">
        <Trajectory events={events} />
      </div>
    </div>
  );
}

