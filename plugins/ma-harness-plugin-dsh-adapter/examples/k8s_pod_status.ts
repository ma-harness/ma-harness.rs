// k8s_pod_status.ts — 完整 dsh (DeepSeek Harness) 插件示例
//
// 业务方: ma-harness 通过 dsh-adapter (P13) 加载这个 dsh 插件, 走 JSON-RPC over stdio.
//
// 用法:
//   1. 装 dsh: npm install -g @deepseek-ai/dsh-sdk-jsonrpc-server
//   2. ma-harness 加载:  mah load-plugin dsh::./examples/k8s_pod_status.ts
//   3. 业务方:  模型调 k8s_pod_status tool, args {namespace: "prod"} -> 返 pod 状态
//
// 完整 dsh 插件契约 (跟 @deepseek-ai/dsh-tools 风格):
//   - export const name = "k8s_pod_status"  (plugin 名)
//   - export const inject = ["tools"]      (Cordis 依赖声明)
//   - export function apply(ctx)            (plugin 安装入口)
//   - ctx.tools.register(defineTool({...})) (注册工具)
//
// dsh runtime (Node.js 22+):
//   - @deepseek-ai/dsh-tools 提供 defineTool / Tool
//   - @deepseek-ai/cordis 提供 Context, inject
//   - JSON-RPC server 走 dsh-sdk-jsonrpc-server 自动启动

import type { Context } from "@deepseek-ai/cordis";
import { defineTool } from "@deepseek-ai/dsh-tools";

// ============================================================================
// Plugin 标识
// ============================================================================

export const name = "k8s-pod-status-demo";
export const inject = ["tools"];

// ============================================================================
// 工具: k8s_pod_status
// ============================================================================
//
// 跟 dsh P11 design doc §defineTool 风格一致 (4 个核心字段):
//   - name + description: 模型看到的工具名 + 描述
//   - parameters: 参数 JSON Schema (defineTool 自动校验)
//   - output.schema: canonical JSON 输出 schema (registry snapshot / freeze)
//   - output.render: 把 output value 渲染成 model-visible content blocks
//   - execute(args, exec): 实际逻辑 (args 校验后, exec 带不可变身份信息)
//
// execute 六条铁律 (P11 文档):
//   1. 参数已校验 (defineTool 跑过 schema 校验)
//   2. 定义注册后不可变 (要热替换, dispose + re-register)
//   3. 执行身份受保护 (exec.callId/name/args/agent/token/signal 不可变)
//   4. 返回 canonical JSON value (不要返 content blocks)
//   5. 响应 exec.signal (cancel 时 cancel 正在做的工作)
//   6. 用 exec.agent 做异步通知

export function apply(ctx: Context) {
  ctx.tools.register(
    defineTool({
      name: "k8s_pod_status",
      description:
        "Check the status of pods in a Kubernetes namespace. Returns pod names, " +
        "status, restarts, and age. Real production use requires `kubectl` in PATH " +
        "and a valid kubeconfig.",
      parameters: {
        namespace: {
          type: "string",
          required: true,
          description: "Kubernetes namespace (e.g. 'production', 'staging')",
        },
        labelSelector: {
          type: "string",
          required: false,
          description: "Optional label selector, e.g. 'app=nginx' or 'tier=backend'",
        },
        timeoutMs: {
          type: "number",
          required: false,
          description: "kubectl exec timeout in ms (default 5000)",
        },
      },
      output: {
        schema: {
          type: "object",
          properties: {
            namespace: { type: "string" },
            pods: {
              type: "array",
              items: {
                type: "object",
                properties: {
                  name: { type: "string" },
                  namespace: { type: "string" },
                  status: { type: "string" },
                  restarts: { type: "number" },
                  age: { type: "string" },
                },
              },
            },
            kubectlOutput: {
              type: "string",
              description: "Raw `kubectl get pods` output (for debugging)",
            },
          },
        },
        render: (_args: unknown, value: unknown) => [
          { type: "text", text: JSON.stringify(value, null, 2) },
        ],
      },
      async execute(args: {
        namespace: string;
        labelSelector?: string;
        timeoutMs?: number;
      }, exec: { signal: AbortSignal }) {
        // 业务方实际跑 kubectl 拿 pod 状态
        // P13 演示: 返 mock 数据 (无 kubectl 依赖, 业务方真用时解开注释)

        // 真实实现 (解开注释用):
        //   const { exec } = await import("node:child_process");
        //   const timeout = args.timeoutMs ?? 5000;
        //   const selector = args.labelSelector ? `-l ${args.labelSelector}` : "";
        //   const { stdout } = await exec.exec(
        //     `kubectl get pods -n ${args.namespace} ${selector} -o json`,
        //     { timeout, signal: exec.signal },
        //   );
        //   const parsed = JSON.parse(stdout);
        //   return {
        //     namespace: args.namespace,
        //     pods: (parsed.items || []).map((p: any) => ({
        //       name: p.metadata.name,
        //       namespace: p.metadata.namespace,
        //       status: p.status.phase,
        //       restarts: p.status.containerStatuses?.reduce(
        //         (s: number, c: any) => s + (c.restartCount || 0), 0) || 0,
        //       age: p.metadata.creationTimestamp,
        //     })),
        //     kubectlOutput: stdout,
        //   };

        // P13 演示 mock 数据
        return {
          namespace: args.namespace,
          pods: [
            {
              name: "nginx-7c8d9f-x7z2k",
              namespace: args.namespace,
              status: "Running",
              restarts: 0,
              age: "2d5h",
            },
            {
              name: "redis-6b9c8d-m4p1q",
              namespace: args.namespace,
              status: "Running",
              restarts: 1,
              age: "5d12h",
            },
          ],
          kubectlOutput:
            "(mock) kubectl get pods would run here with namespace=" +
            args.namespace +
            (args.labelSelector ? ` labelSelector=${args.labelSelector}` : ""),
        };
      },
    })
  );
}
