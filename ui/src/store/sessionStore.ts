import { create } from 'zustand';
import type { ProtoEvent, ProtoSession } from '@/api/types';

// 2026-08-19 (Day 101 / P7-1.1): 全局 session store (Zustand)
// 选 Zustand 不用 Redux, 适合 dashboard 这种轻量场景
// 业务方: 跨页面共享 session 状态时, 调 useSessionStore.getState() / useSessionStore(selector)

interface SessionState {
  // 选中的 session id
  selectedSessionId: string | null;
  setSelectedSessionId: (id: string | null) => void;

  // 当前 session 详情 cache
  session: ProtoSession | null;
  setSession: (s: ProtoSession | null) => void;

  // session events 累积 (Trajectory 视图用)
  events: ProtoEvent[];
  appendEvents: (events: ProtoEvent[]) => void;
  clearEvents: () => void;

  // 上次手动保存的 session id (P6-5 类似, 重启 UI 自动恢复)
  lastSessionId: string | null;
  setLastSessionId: (id: string | null) => void;
}

export const useSessionStore = create<SessionState>((set) => ({
  selectedSessionId: null,
  setSelectedSessionId: (id) => set({ selectedSessionId: id }),

  session: null,
  setSession: (s) => set({ session: s }),

  events: [],
  appendEvents: (newEvents) =>
    set((state) => ({ events: [...state.events, ...newEvents] })),
  clearEvents: () => set({ events: [] }),

  lastSessionId: null,
  setLastSessionId: (id) => set({ lastSessionId: id }),
}));
