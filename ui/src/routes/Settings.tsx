import { useState } from 'react';
import { Save } from 'lucide-react';
import { cn } from '@/lib/utils';

// 2026-08-19 (Day 101 / P7-1.6): Settings 页面
// - Workspace 路径 (UI 启动时 agent 操作的目录)
// - API Key (OpenAI / Anthropic, 写到 localStorage 不上传 server)
// - 模型选择 (跟 ma-harness-model adapter 列表)
// 业务方: P7-1.6 完整版会加更多 (theme / 快捷键 / telemetry 开关)

export default function Settings() {
  const [workspace, setWorkspace] = useState(localStorage.getItem('mah_workspace') || '');
  const [apiKey, setApiKey] = useState(localStorage.getItem('mah_api_key') || '');
  const [model, setModel] = useState(localStorage.getItem('mah_model') || 'stub');
  const [saved, setSaved] = useState(false);

  const handleSave = () => {
    localStorage.setItem('mah_workspace', workspace);
    localStorage.setItem('mah_api_key', apiKey);
    localStorage.setItem('mah_model', model);
    setSaved(true);
    setTimeout(() => setSaved(false), 2000);
  };

  return (
    <div className="mx-auto max-w-2xl p-6">
      <h1 className="mb-6 text-lg font-semibold">Settings</h1>

      <Section title="Workspace" description="Default directory the agent will operate in.">
        <input
          type="text"
          value={workspace}
          onChange={(e) => setWorkspace(e.target.value)}
          placeholder="/path/to/your/project"
          className="w-full rounded border border-border bg-bg-panel px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-border-accent"
        />
      </Section>

      <Section title="API Key" description="Provider API key. Stored in browser localStorage only (never sent to ma-harness server).">
        <input
          type="password"
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="sk-..."
          className="w-full rounded border border-border bg-bg-panel px-3 py-1.5 text-sm text-fg placeholder:text-fg-muted focus:border-border-accent"
        />
      </Section>

      <Section title="Model" description="Default model adapter for new runs.">
        <select
          value={model}
          onChange={(e) => setModel(e.target.value)}
          className="w-full rounded border border-border bg-bg-panel px-3 py-1.5 text-sm text-fg focus:border-border-accent"
        >
          <option value="stub">stub (no LLM, test only)</option>
          <option value="openai">openai (gpt-4o, gpt-4-turbo, etc.)</option>
          <option value="anthropic">anthropic (claude-3.5-sonnet, etc.)</option>
          <option value="openai-azure">openai-azure (Azure OpenAI Service)</option>
        </select>
      </Section>

      <div className="mt-6 flex items-center gap-3">
        <button
          onClick={handleSave}
          className="flex items-center gap-1 rounded border border-border-accent bg-border-accent px-4 py-1.5 text-sm text-white hover:opacity-90"
        >
          <Save className="h-3 w-3" />
          Save
        </button>
        {saved && <span className="text-xs text-success">Saved</span>}
      </div>
    </div>
  );
}

function Section({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <section className="mb-5">
      <label className="mb-1 block text-sm font-medium">{title}</label>
      <p className={cn('mb-2 text-xs text-fg-muted')}>{description}</p>
      {children}
    </section>
  );
}
