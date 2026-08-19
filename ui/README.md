# ma-harness Web UI

Web dashboard for ma-harness — React + Vite + TypeScript, port 3080.

## 启动

```bash
cd ui
pnpm install       # 装依赖 (Node 20+)
pnpm dev           # 启动 dev server, http://localhost:3080
```

## 跟 ma-harness-server 集成

Vite dev server proxy 配置 (`vite.config.ts`):

- `/api/*` → `http://localhost:50050/*` (gRPC-web 桥, 走 tonic-web)
- 业务方访问: `http://localhost:3080/api/v1/sessions` → 转到 tonic 50050

要 Web UI 工作, 必须先启动 `mah start` (跑 tonic gRPC :50050 + salvo HTTP :50050)。

## 项目结构

```
ui/
├── src/
│   ├── main.tsx              # React 入口
│   ├── App.tsx               # 顶层路由
│   ├── index.css             # TailwindCSS 入口
│   ├── routes/
│   │   ├── Sessions.tsx     # /sessions — session 列表 (P7-1.3)
│   │   ├── SessionDetail.tsx # /sessions/:id — detail + run + trajectory (P7-1.3+1.4+1.7)
│   │   └── Settings.tsx      # /settings — workspace + API key + model (P7-1.6)
│   ├── components/
│   │   └── Trajectory.tsx    # System/User/Assistant/Tool 时间线 (P7-1.4)
│   ├── api/
│   │   ├── grpc.ts           # gRPC-web client wrapper (P7-1.2)
│   │   └── types.ts          # Proto 类型 (跟 ma-harness-proto 对齐)
│   ├── store/
│   │   └── sessionStore.ts   # Zustand 全局 session state
│   └── lib/
│       └── utils.ts          # cn 工具
├── vite.config.ts            # port 3080 + proxy /api → tonic :50050
├── tailwind.config.js        # 暗色 dashboard 主题
├── tsconfig.json             # strict TypeScript
├── package.json
└── index.html
```

## Phase 7 任务对应

| 任务 | 状态 | 文件 |
|---|---|---|
| P7-1.1 项目初始化 | ✅ | `package.json` / `vite.config.ts` / `tsconfig.json` |
| P7-1.2 tonic-web 集成 | ⏳ | `src/api/grpc.ts` (placeholder) |
| P7-1.3 Session 列表 | ✅ 简化版 | `src/routes/Sessions.tsx` |
| P7-1.4 Trajectory 视图 | ✅ 简化版 | `src/components/Trajectory.tsx` |
| P7-1.5 Token 监控 widget | ⏳ | (待 P7-1.5) |
| P7-1.6 Settings 页面 | ✅ 简化版 | `src/routes/Settings.tsx` |
| P7-1.7 实时事件推送 | ✅ EventSource stub | `src/api/grpc.ts:streamSessionEvents` |
| P7-1.8 Playwright e2e | ⏳ | (待 P7-1.8) |

## 业务方集成指南

### 1. 启动 Web UI

```bash
# Terminal 1
mah start
# tonic gRPC :50050 + salvo HTTP :50050 起来

# Terminal 2
cd ui
pnpm dev
# Web UI http://localhost:3080 起来, proxy /api → :50050
```

### 2. 自定义 UI 组件

- `@/components/Trajectory.tsx` 改时间线样式
- `@/routes/Sessions.tsx` 改 session 列表布局
- `@/lib/utils.ts` 拿 cn() 工具

### 3. 加新 route

`src/App.tsx` 加 `<Route>`, 跟 P7-1.3 / 1.4 / 1.6 一样在 `src/routes/` 加新文件。

### 4. 加新 gRPC API

`src/api/types.ts` 加 type, `src/api/grpc.ts` 加 fetch 函数, 业务方在 component 用 `useQuery` 拿。

## 给后来人

- React 18 + Vite 5 + TypeScript 5 + TailwindCSS 3 + TanStack Query 5 + Zustand 4
- 暗色 dashboard 主题, 跟 dsh / TRAE 类似
- 业务方必须先 `mah start` 起 tonic 才能用 Web UI (proxy 依赖)
- 完整 e2e 测试 (P7-1.8) 用 Playwright, 跑 `pnpm test:e2e`
- 业务方改 proto 后, 同步改 `src/api/types.ts` 跟 `src/api/grpc.ts`
- TailwindCSS 主题色在 `tailwind.config.js`, 改 primary / bg / fg 一次到位
