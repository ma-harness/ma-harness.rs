import { NavLink, Navigate, Route, Routes } from 'react-router-dom';
import { Suspense, lazy } from 'react';
import { Settings as SettingsIcon, Activity } from 'lucide-react';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P7-1.1): 顶层 App 组件
// - 路由: /sessions (default) / /sessions/:id / /settings
// - 顶部 nav + 主区 (Suspense 懒加载 P7-1.3/4/5/6/7/8 的重组件)
const Sessions = lazy(() => import('@/routes/Sessions'));
const SessionDetail = lazy(() => import('@/routes/SessionDetail'));
const Settings = lazy(() => import('@/routes/Settings'));

export default function App() {
  return (
    <div className="flex h-full flex-col">
      {/* Top nav bar */}
      <header className="flex h-12 items-center gap-6 border-b border-border bg-bg-panel px-6">
        <div className="flex items-center gap-2 text-fg-accent">
          <Activity className="h-5 w-5" />
          <span className="font-mono text-sm font-semibold">ma-harness</span>
        </div>
        <nav className="flex items-center gap-4 text-sm">
          <NavLink
            to="/sessions"
            className={({ isActive }) =>
              cn('transition-colors hover:text-fg-accent', isActive ? 'text-fg-accent' : 'text-fg-muted')
            }
          >
            Sessions
          </NavLink>
        </nav>
        <div className="ml-auto">
          <NavLink
            to="/settings"
            className={({ isActive }) =>
              cn('flex items-center gap-1 transition-colors hover:text-fg-accent', isActive ? 'text-fg-accent' : 'text-fg-muted')
            }
          >
            <SettingsIcon className="h-4 w-4" />
            <span>Settings</span>
          </NavLink>
        </div>
      </header>

      {/* Main area */}
      <main className="flex-1 overflow-auto">
        <Suspense fallback={<div className="p-6 text-fg-muted">Loading…</div>}>
          <Routes>
            <Route path="/" element={<Navigate to="/sessions" replace />} />
            <Route path="/sessions" element={<Sessions />} />
            <Route path="/sessions/:id" element={<SessionDetail />} />
            <Route path="/settings" element={<Settings />} />
            <Route path="*" element={<div className="p-6">404 — Not Found</div>} />
          </Routes>
        </Suspense>
      </main>
    </div>
  );
}
