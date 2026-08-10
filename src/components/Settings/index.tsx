// Settings panel — configuration UI for VoiceBoom
// Tabs: Voice, AI Model, Shortcuts, Display, Advanced, About

import { useState, useEffect } from 'react';
import { motion } from 'framer-motion';
import { useAppStore, type AsrEngineType } from '../../stores/useAppStore';
import { invoke } from '@tauri-apps/api/core';

type TabId = 'voice' | 'model' | 'local' | 'shortcuts' | 'display' | 'advanced' | 'about';

interface Tab {
  id: TabId;
  label: string;
}

const TABS: Tab[] = [
  { id: 'voice', label: '语音' },
  { id: 'model', label: 'AI 模型' },
  { id: 'local', label: '本地资源' },
  { id: 'shortcuts', label: '快捷键' },
  { id: 'display', label: '显示' },
  { id: 'advanced', label: '高级' },
  { id: 'about', label: '关于' },
];

/// Engine metadata for UI rendering
interface EngineInfo {
  id: AsrEngineType;
  name: string;
  description: string;
  keyPlaceholder: string;
  keyHelp: string;
  endpointPlaceholder: string;
  isLocal: boolean;
  downloadUrl?: string;
  downloadHelp?: string;
}

const ENGINES: EngineInfo[] = [
  {
    id: 'openai_whisper',
    name: 'OpenAI Whisper API',
    description: 'OpenAI 官方云端语音识别，支持多语言，准确率高',
    keyPlaceholder: 'sk-xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 platform.openai.com/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.openai.com/v1/audio/transcriptions',
    isLocal: false,
  },
  {
    id: 'deepgram',
    name: 'Deepgram',
    description: '专业语音识别服务，低延迟流式转写',
    keyPlaceholder: 'xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 console.deepgram.com/settings/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.deepgram.com/v1/listen',
    isLocal: false,
  },
  {
    id: 'whisper_cpp',
    name: 'Whisper.cpp（本地）',
    description: '本地离线运行，无需网络，保护隐私',
    keyPlaceholder: '（本地服务无需 API Key）',
    keyHelp: '本地模型不需要 API Key，但需要下载模型文件',
    endpointPlaceholder: 'ws://localhost:8080/ws',
    isLocal: true,
    downloadUrl: 'https://github.com/ggerganov/whisper.cpp',
    downloadHelp: '下载 whisper.cpp 并运行本地 WebSocket 服务',
  },
  {
    id: 'funasr',
    name: 'FunASR（本地）',
    description: '阿里达摩院中文语音识别，本地离线运行',
    keyPlaceholder: '（本地服务无需 API Key）',
    keyHelp: '本地模型不需要 API Key，但需要下载模型文件',
    endpointPlaceholder: 'ws://localhost:9880/ws',
    isLocal: true,
    downloadUrl: 'https://github.com/modelscope/FunASR',
    downloadHelp: '下载 FunASR 并运行本地 WebSocket 服务',
  },
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
  disabled = false,
  helpText,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  helpText?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm text-gray-600">{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className={`rounded-lg border border-gray-200 px-3 py-2 text-sm bg-white focus:outline-none focus:ring-2 focus:ring-blue-400 ${
          disabled ? 'opacity-50 cursor-not-allowed bg-gray-50' : ''
        }`}
      />
      {helpText && <p className="text-xs text-gray-400 mt-0.5">{helpText}</p>}
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
        label="识别语言"
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
        value={settings.vadSensitivity}
        min={0}
        max={100}
        onChange={(v) => updateSettings({ vadSensitivity: v })}
      />
      <p className="text-xs text-gray-400">
        灵敏度越高，越容易检测到语音开始；灵敏度越低，越不容易被环境噪音误触发。
      </p>
    </div>
  );
}

/// Tab content: AI Model settings — redesigned with engine-specific fields
function ModelTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);

  const currentEngine = ENGINES.find((e) => e.id === settings.engine) || ENGINES[0];

  const handleEngineChange = (engineId: string) => {
    updateSettings({ engine: engineId as AsrEngineType });
  };

  return (
    <div className="flex flex-col gap-5">
      {/* Engine Selection */}
      <div className="flex flex-col gap-2">
        <label className="text-sm text-gray-600 font-medium">语音识别服务</label>
        <div className="grid grid-cols-1 gap-2">
          {ENGINES.map((engine) => (
            <button
              key={engine.id}
              onClick={() => handleEngineChange(engine.id)}
              className={`text-left p-3 rounded-lg border-2 transition-all ${
                settings.engine === engine.id
                  ? 'border-blue-500 bg-blue-50'
                  : 'border-gray-200 hover:border-gray-300 bg-white'
              }`}
            >
              <div className="flex items-center justify-between">
                <span className="text-sm font-medium text-gray-800">{engine.name}</span>
                {engine.isLocal && (
                  <span className="text-xs px-2 py-0.5 bg-green-100 text-green-700 rounded-full">
                    本地离线
                  </span>
                )}
                {settings.engine === engine.id && (
                  <span className="text-blue-500">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="20 6 9 17 4 12"/>
                    </svg>
                  </span>
                )}
              </div>
              <p className="text-xs text-gray-500 mt-1">{engine.description}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Engine-specific configuration */}
      <div className="border-t border-gray-200 pt-4">
        <div className="flex items-center gap-2 mb-3">
          <span className="text-sm font-medium text-gray-700">
            {currentEngine.name} 配置
          </span>
          {currentEngine.isLocal && (
            <span className="text-xs px-2 py-0.5 bg-amber-100 text-amber-700 rounded-full">
              需本地部署
            </span>
          )}
        </div>

        {/* API Key (cloud services only) */}
        {!currentEngine.isLocal ? (
          <div className="flex flex-col gap-3">
            <TextInput
              label={`${currentEngine.name} API Key`}
              value={settings.apiKey}
              onChange={(v) => updateSettings({ apiKey: v })}
              placeholder={currentEngine.keyPlaceholder}
              type="password"
              helpText={currentEngine.keyHelp}
            />
            <TextInput
              label="API 端点（可选）"
              value={settings.endpoint}
              onChange={(v) => updateSettings({ endpoint: v })}
              placeholder={currentEngine.endpointPlaceholder}
              helpText="留空使用默认端点"
            />
          </div>
        ) : (
          <div className="flex flex-col gap-3">
            {/* Local service endpoint */}
            <TextInput
              label="本地服务地址"
              value={settings.endpoint}
              onChange={(v) => updateSettings({ endpoint: v })}
              placeholder={currentEngine.endpointPlaceholder}
              helpText="本地 WebSocket 服务地址"
            />
            {/* Download link */}
            {currentEngine.downloadUrl && (
              <div className="p-3 bg-amber-50 rounded-lg border border-amber-200">
                <div className="flex items-center justify-between">
                  <div>
                    <p className="text-sm font-medium text-amber-800">需要下载本地服务</p>
                    <p className="text-xs text-amber-600 mt-0.5">{currentEngine.downloadHelp}</p>
                  </div>
                  <a
                    href={currentEngine.downloadUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="px-3 py-1.5 bg-amber-500 text-white text-xs rounded-lg hover:bg-amber-600 transition-colors whitespace-nowrap"
                  >
                    前往下载
                  </a>
                </div>
              </div>
            )}
            <TextInput
              label="API Key"
              value={settings.apiKey}
              onChange={(v) => updateSettings({ apiKey: v })}
              placeholder={currentEngine.keyPlaceholder}
              disabled={true}
              helpText={currentEngine.keyHelp}
            />
          </div>
        )}
      </div>

      {/* Connection status indicator */}
      <div className="border-t border-gray-200 pt-4">
        <div className="flex items-center gap-2">
          <div className={`w-2 h-2 rounded-full ${settings.apiKey || currentEngine.isLocal ? 'bg-green-500' : 'bg-red-500'}`} />
          <span className="text-xs text-gray-500">
            {settings.apiKey || currentEngine.isLocal
              ? '已配置，可以开始语音识别'
              : '请填写 API Key 后使用'}
          </span>
        </div>
      </div>
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
      <div className="p-3 bg-blue-50 rounded-lg border border-blue-200">
        <p className="text-xs text-blue-600">
          💡 提示：如果快捷键与系统冲突，推荐使用 Ctrl+Shift+V 或 Alt+Space
        </p>
      </div>
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
      <div className="text-xs text-gray-400 mt-2 space-y-1">
        <p>数据存储位置: %APPDATA%\com.voiceboom.app\</p>
        <p>日志级别: INFO</p>
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
      <div className="text-xs text-gray-400 mt-4 space-y-1 text-center">
        <p>React 19 + Tauri 2.0 + Rust</p>
        <p>OpenAI Whisper / Deepgram / Whisper.cpp / FunASR</p>
      </div>
    </div>
  );
}

/// Tab content: Local Resources management
function LocalResourcesTab() {
  const [resources, setResources] = useState<any[]>([]);
  const [selectedEngine, setSelectedEngine] = useState<string | null>(null);

  useEffect(() => {
    invoke('get_resource_status').then((status) => {
      setResources(status as any[]);
    }).catch(() => {});
  }, []);

  const handleInstall = async (engine: string) => {
    // Use a simple prompt for the path (dialog plugin not available in this config)
    const path = prompt(`请输入 ${engine} 资源包目录的完整路径:\n\n例如: C:\\VoiceBoom\\resources\\whisper_cpp`);
    if (path && path.trim()) {
      invoke('install_resource', {
        engine,
        sourcePath: path.trim(),
        version: '1.0.0',
        channel: 'stable',
      }).then(() => {
        // Refresh status
        invoke('get_resource_status').then((status) => {
          setResources(status as any[]);
        });
      }).catch((e) => {
        alert('安装失败: ' + e);
      });
    }
  };

  const handleRemove = async (engine: string) => {
    invoke('remove_resource', { engine }).then(() => {
      invoke('get_resource_status').then((status) => {
        setResources(status as any[]);
      });
    }).catch(() => {});
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  const engines = [
    { id: 'whisper_cpp', name: 'Whisper.cpp', description: '本地离线语音识别，支持多语言' },
    { id: 'funasr', name: 'FunASR', description: '阿里达摩院中文语音识别，离线运行' },
  ];

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-gray-500">
        管理本地语音识别服务的资源包。资源包包含运行本地推理所需的模型文件和服务器程序。
      </p>

      {engines.map((engine) => {
        const resource = resources.find((r) => r.engine === engine.id);
        const isReady = resource?.is_ready;

        return (
          <div key={engine.id} className="p-4 rounded-lg border border-gray-200 bg-white">
            <div className="flex items-center justify-between">
              <div>
                <h3 className="text-sm font-medium text-gray-800">{engine.name}</h3>
                <p className="text-xs text-gray-500 mt-0.5">{engine.description}</p>
              </div>
              {isReady ? (
                <span className="text-xs px-2 py-1 bg-green-100 text-green-700 rounded-full">已就绪</span>
              ) : (
                <span className="text-xs px-2 py-1 bg-gray-100 text-gray-500 rounded-full">未安装</span>
              )}
            </div>

            {resource && isReady && (
              <div className="mt-3 pt-3 border-t border-gray-100">
                <div className="grid grid-cols-2 gap-2 text-xs text-gray-500">
                  <div>版本: <span className="text-gray-700">{resource.version}</span></div>
                  <div>大小: <span className="text-gray-700">{formatSize(resource.size_bytes)}</span></div>
                  <div>通道: <span className="text-gray-700">{resource.channel_name}</span></div>
                  <div>路径: <span className="text-gray-700 truncate block" title={resource.path}>{resource.path}</span></div>
                </div>
              </div>
            )}

            <div className="mt-3 flex gap-2">
              {!isReady ? (
                <button
                  onClick={() => handleInstall(engine.id)}
                  className="px-3 py-1.5 bg-blue-500 text-white text-xs rounded-lg hover:bg-blue-600 transition-colors"
                >
                  安装资源包...
                </button>
              ) : (
                <>
                  <button
                    onClick={() => handleInstall(engine.id)}
                    className="px-3 py-1.5 bg-gray-100 text-gray-700 text-xs rounded-lg hover:bg-gray-200 transition-colors"
                  >
                    更新/更换
                  </button>
                  <button
                    onClick={() => handleRemove(engine.id)}
                    className="px-3 py-1.5 bg-red-50 text-red-600 text-xs rounded-lg hover:bg-red-100 transition-colors"
                  >
                    卸载
                  </button>
                </>
              )}
            </div>
          </div>
        );
      })}

      <div className="p-3 bg-blue-50 rounded-lg border border-blue-200">
        <p className="text-xs text-blue-600">
          💡 提示：安装资源包时，请选择包含服务器程序和模型文件的目录。
          后续版本将支持在线下载和版本通道选择（稳定版/前瞻版）。
        </p>
      </div>
    </div>
  );
}

/// Main settings panel
export function SettingsPanel() {
  const [activeTab, setActiveTab] = useState<TabId>('model'); // Default to model tab for first-time setup

  const renderTab = () => {
    switch (activeTab) {
      case 'voice':
        return <VoiceTab />;
      case 'model':
        return <ModelTab />;
      case 'local':
        return <LocalResourcesTab />;
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
