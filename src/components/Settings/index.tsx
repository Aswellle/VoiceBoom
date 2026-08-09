// Settings panel — configuration UI for VoiceBoom
// Tabs: Voice, AI Model, Shortcuts, Display, Advanced, About

import { useState } from 'react';
import { motion } from 'framer-motion';
import { useAppStore, type AsrEngineType } from '../../stores/useAppStore';

type TabId = 'voice' | 'model' | 'shortcuts' | 'display' | 'advanced' | 'about';

interface Tab {
  id: TabId;
  label: string;
}

const TABS: Tab[] = [
  { id: 'voice', label: '语音' },
  { id: 'model', label: 'AI 模型' },
  { id: 'shortcuts', label: '快捷键' },
  { id: 'display', label: '显示' },
  { id: 'advanced', label: '高级' },
  { id: 'about', label: '关于' },
];

/// Slider component
function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  onChange,
  unit = '',
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  unit?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-between text-sm">
        <span className="text-gray-600">{label}</span>
        <span className="text-gray-400">
          {value}
          {unit}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-blue-500"
      />
    </div>
  );
}

/// Select component
function Select({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-gray-600">{label}</label>
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-lg border border-gray-200 px-3 py-2 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-blue-400"
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/// Text input component
function TextInput({
  label,
  value,
  onChange,
  placeholder,
  type = 'text',
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-gray-600">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="rounded-lg border border-gray-200 px-3 py-2 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-blue-400"
      />
    </div>
  );
}

/// Toggle component
function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between cursor-pointer">
      <span className="text-sm text-gray-600">{label}</span>
      <div
        className={`
          relative w-10 h-5 rounded-full transition-colors
          ${checked ? 'bg-blue-500' : 'bg-gray-300'}
        `}
        onClick={() => onChange(!checked)}
      >
        <div
          className={`
            absolute top-0.5 w-4 h-4 rounded-full bg-white shadow transition-transform
            ${checked ? 'translate-x-5' : 'translate-x-0.5'}
          `}
        />
      </div>
    </label>
  );
}

/// Tab content: Voice settings
function VoiceTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <div className="flex flex-col gap-4">
      <Select
        label="语言"
        value={settings.language}
        onChange={(v) => updateSettings({ language: v })}
        options={[
          { value: 'auto', label: '自动检测' },
          { value: 'zh', label: '中文（普通话）' },
          { value: 'en', label: 'English' },
          { value: 'ja', label: '日本語' },
          { value: 'ko', label: '한국어' },
        ]}
      />
      <Slider
        label="VAD 灵敏度"
        value={50}
        min={0}
        max={100}
        onChange={() => {}}
      />
    </div>
  );
}

/// Tab content: AI Model settings
function ModelTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <div className="flex flex-col gap-4">
      <Select
        label="ASR 引擎"
        value={settings.engine}
        onChange={(v) => updateSettings({ engine: v as AsrEngineType })}
        options={[
          { value: 'openai_whisper', label: 'OpenAI Whisper API' },
          { value: 'deepgram', label: 'Deepgram' },
          { value: 'whisper_cpp', label: 'Whisper.cpp (本地)' },
          { value: 'funasr', label: 'FunASR (本地)' },
        ]}
      />
      <TextInput
        label="API Key"
        value={settings.apiKey}
        onChange={(v) => updateSettings({ apiKey: v })}
        placeholder="输入您的 API Key"
        type="password"
      />
      <TextInput
        label="API 端点（可选）"
        value={settings.endpoint}
        onChange={(v) => updateSettings({ endpoint: v })}
        placeholder="留空使用默认端点"
      />
    </div>
  );
}

/// Tab content: Shortcut settings
function ShortcutsTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <div className="flex flex-col gap-4">
      <TextInput
        label="录音快捷键"
        value={settings.shortcut}
        onChange={(v) => updateSettings({ shortcut: v })}
        placeholder="例如: Ctrl+Space"
      />
      <p className="text-xs text-gray-400">
        按住快捷键开始录音，松开停止。支持 Ctrl、Alt、Shift、Cmd 等修饰键组合。
      </p>
    </div>
  );
}

/// Tab content: Display settings
function DisplayTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <div className="flex flex-col gap-4">
      <Slider
        label="最大展示字符数"
        value={settings.maxChars}
        min={20}
        max={500}
        step={10}
        unit=" 字符"
        onChange={(v) => updateSettings({ maxChars: v })}
      />
      <Slider
        label="字体大小"
        value={settings.fontSize}
        min={14}
        max={36}
        unit="px"
        onChange={(v) => updateSettings({ fontSize: v })}
      />
      <Slider
        label="窗口透明度"
        value={Math.round(settings.opacity * 100)}
        min={30}
        max={100}
        unit="%"
        onChange={(v) => updateSettings({ opacity: v / 100 })}
      />
      <Select
        label="主题"
        value={settings.theme}
        onChange={(v) => updateSettings({ theme: v as 'auto' | 'light' | 'dark' })}
        options={[
          { value: 'auto', label: '跟随系统' },
          { value: 'light', label: '亮色模式' },
          { value: 'dark', label: '暗色模式' },
        ]}
      />
      <Toggle
        label="减弱动画（无障碍）"
        checked={settings.reduceMotion}
        onChange={(v) => updateSettings({ reduceMotion: v })}
      />
    </div>
  );
}

/// Tab content: Advanced settings
function AdvancedTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        label="开机自启"
        checked={settings.autoStart}
        onChange={(v) => updateSettings({ autoStart: v })}
      />
      <div className="text-xs text-gray-400 mt-2">
        <p>数据存储位置: ~/.voiceboom/</p>
        <p className="mt-1">日志级别: INFO</p>
      </div>
    </div>
  );
}

/// Tab content: About
function AboutTab() {
  return (
    <div className="flex flex-col items-center gap-4 py-8">
      <div className="text-4xl">🎙️</div>
      <h2 className="text-xl font-semibold text-gray-800">VoiceBoom AI</h2>
      <p className="text-sm text-gray-500">版本 0.1.0 (MVP)</p>
      <p className="text-xs text-gray-400 text-center max-w-xs">
        实时流式智能语音输入法 — 像 Apple macOS 原生交互一样优雅，
        同时具备 AI 时代实时语音输入能力。
      </p>
      <div className="text-xs text-gray-400 mt-4">
        <p>React 19 + Tauri 2.0 + Rust</p>
        <p>OpenAI Whisper / Deepgram</p>
      </div>
    </div>
  );
}

/// Main settings panel
export function SettingsPanel() {
  const [activeTab, setActiveTab] = useState<TabId>('voice');

  const renderTab = () => {
    switch (activeTab) {
      case 'voice':
        return <VoiceTab />;
      case 'model':
        return <ModelTab />;
      case 'shortcuts':
        return <ShortcutsTab />;
      case 'display':
        return <DisplayTab />;
      case 'advanced':
        return <AdvancedTab />;
      case 'about':
        return <AboutTab />;
    }
  };

  return (
    <div className="flex h-full bg-gray-50">
      {/* Sidebar tabs */}
      <nav className="w-40 bg-white border-r border-gray-200 py-4">
        {TABS.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`
              w-full text-left px-4 py-2.5 text-sm transition-colors
              ${
                activeTab === tab.id
                  ? 'bg-blue-50 text-blue-600 font-medium'
                  : 'text-gray-600 hover:bg-gray-50'
              }
            `}
          >
            {tab.label}
          </button>
        ))}
      </nav>

      {/* Tab content */}
      <main className="flex-1 p-6 overflow-y-auto">
        <motion.div
          key={activeTab}
          initial={{ opacity: 0, y: 5 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15 }}
        >
          {renderTab()}
        </motion.div>
      </main>
    </div>
  );
}
