import { useNavigate } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { fetchSessionList } from '@/api/grpc';
import type { ProtoSession } from '@/api/types';
import { Plus, RefreshCw } from 'lucide-react';
import { useState } from 'react';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P7-1.3): Session 列表页
// - TanStack Query 拿 session list (15s 刷新)
// - 点击行 → navigate /sessions/:id (P7-1.3 / 1.4 detail view)
// - "+ New Session" 按钮 → 创建表单 (后续 P7-1.6 集成 Settings)

export default function Sessions() {
  const navigate = useNavigate();
  const [search, setSearch] = useState('');

  const { data, isLoading, error, refetch, isFetching } = useQuery({
    queryKey: ['sessions'],
    queryFn: () => fetchSessionList({ page_size: 50, page_token: '' }),
    refetchInterval: 15_000,
  });

  const sessions = (data?.sessions || []).filter(
    (s) => !search || s.id.includes(search) || s.name.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div className="p-6">
      <div className="mb-4 flex items-center gap-3">
        <h1 className="text-lg font-semibold">Sessions</h1>
        <span className="text-xs text-fg-muted">
          {data?.sessions.length ?? 0} total
        </span>
        <div className="ml-auto flex items-center gap-2">
          <input
            type="text"
            placeholder="search…"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="rounded border border-border bg-bg-panel px-3 py-1 text-sm text-fg placeholder:text-fg-muted focus:border-border-accent"
          />
          <button
            onClick={() => refetch()}
            className={cn(
              'flex items-center gap-1 rounded border border-border bg-bg-panel px-3 py-1 text-sm hover:bg-bg-hover',
              isFetching && 'opacity-60',
            )}
          >
            <RefreshCw className="h-3 w-3" />
            Refresh
          </button>
          <button
            // TODO(P7-1.6): 打开 create session dialog
            className="flex items-center gap-1 rounded border border-border-accent bg-border-accent px-3 py-1 text-sm text-white hover:opacity-90"
          >
            <Plus className="h-3 w-3" />
            New
          </button>
        </div>
      </div>

      {error && (
        <div className="mb-4 rounded border border-error bg-bg-panel p-3 text-sm text-error">
          Error: {(error as Error).message}
        </div>
      )}

      {isLoading ? (
        <div className="text-fg-muted">Loading…</div>
      ) : (
        <div className="overflow-hidden rounded border border-border bg-bg-panel">
          <table className="w-full text-sm">
            <thead className="border-b border-border bg-bg">
              <tr className="text-fg-muted">
                <th className="px-4 py-2 text-left font-medium">ID</th>
                <th className="px-4 py-2 text-left font-medium">Name</th>
                <th className="px-4 py-2 text-left font-medium">State</th>
                <th className="px-4 py-2 text-left font-medium">Plugins</th>
                <th className="px-4 py-2 text-left font-medium">Created</th>
              </tr>
            </thead>
            <tbody>
              {sessions.length === 0 ? (
                <tr>
                  <td colSpan={5} className="px-4 py-8 text-center text-fg-muted">
                    {search ? 'No matches' : 'No sessions yet — click "New" to create one'}
                  </td>
                </tr>
              ) : (
                sessions.map((s) => (
                  <SessionRow key={s.id} session={s} onClick={() => navigate(`/sessions/${s.id}`)} />
                ))
              )}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

function SessionRow({ session, onClick }: { session: ProtoSession; onClick: () => void }) {
  return (
    <tr
      onClick={onClick}
      className="cursor-pointer border-b border-border last:border-b-0 hover:bg-bg-hover"
    >
      <td className="px-4 py-2 font-mono text-xs">{session.id.slice(0, 8)}</td>
      <td className="px-4 py-2">{session.name || <span className="text-fg-muted">—</span>}</td>
      <td className="px-4 py-2">
        <StateBadge state={session.state} />
      </td>
      <td className="px-4 py-2 text-xs text-fg-muted">
        {session.enabled_plugins.join(', ') || '—'}
      </td>
      <td className="px-4 py-2 text-xs text-fg-muted">
        {new Date(session.created_at).toLocaleString()}
      </td>
    </tr>
  );
}

function StateBadge({ state }: { state: number }) {
  const map: Record<number, { label: string; color: string }> = {
    0: { label: '—', color: 'text-fg-muted' },
    1: { label: 'Created', color: 'text-fg-accent' },
    2: { label: 'Active', color: 'text-success' },
    3: { label: 'Paused', color: 'text-warn' },
    4: { label: 'Closed', color: 'text-fg-muted' },
    5: { label: 'Errored', color: 'text-error' },
    6: { label: 'Cancelled', color: 'text-fg-muted' },
  };
  const { label, color } = map[state] || map[0];
  return <span className={cn('text-xs font-medium', color)}>{label}</span>;
}
