// Settings panel — configuration UI for VoiceBoom
// Tabs: Voice, AI Model, Shortcuts, Display, Advanced, About

import { useState, useEffect, useCallback } from 'react';
import { motion } from 'framer-motion';
import { useAppStore, type AsrEngineType } from '../../stores/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { open as openUrl } from '@tauri-apps/plugin-shell';

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
    name: 'Whisper（本地）',
    description: '多语言离线引擎，当前版本暂不可用，后续版本将整合',
    keyPlaceholder: '（本地引擎无需 API Key）',
    keyHelp: '本地引擎不需要 API Key',
    endpointPlaceholder: '（暂不可用）',
    isLocal: true,
    downloadUrl: 'https://github.com/ggerganov/whisper.cpp',
    downloadHelp: '当前版本使用 SenseVoice，Whisper 支持将在后续版本加入',
  },
  {
    id: 'funasr',
    name: 'SenseVoice（本地）',
    description: '阿里达摩院多语言引擎，内置离线运行，中文识别优秀',
    keyPlaceholder: '（本地引擎无需 API Key）',
    keyHelp: '本地引擎不需要 API Key',
    endpointPlaceholder: '（自动配置）',
    isLocal: true,
    downloadUrl: 'https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17',
    downloadHelp: '模型已内置，开箱即用',
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
      <div className="flex justify-between gap-3 text-sm">
        <span className="text-gray-600 min-w-0">{label}</span>
        <span className="text-gray-400 shrink-0 whitespace-nowrap">
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
    <label className="flex items-center justify-between gap-3 cursor-pointer">
      <span className="text-sm text-gray-600 min-w-0">{label}</span>
      <div
        className={`
          relative w-10 h-5 shrink-0 rounded-full transition-colors
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
  const showToast = useAppStore((s) => s.showToast);
  const [devices, setDevices] = useState<Array<{ id: string; label: string }>>([]);
  const [loadingDevices, setLoadingDevices] = useState(false);

  const refreshDevices = useCallback(() => {
    setLoadingDevices(true);
    invoke<Array<[string, string]>>('get_audio_devices')
      .then((list) => {
        setDevices(list.map(([id, label]) => ({ id, label })));
      })
      .catch(() => {
        // Non-fatal: the dropdown just won't appear.
        setDevices([]);
      })
      .finally(() => setLoadingDevices(false));
  }, []);

  useEffect(() => {
    refreshDevices();
  }, [refreshDevices]);

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
      <div className="flex flex-col gap-1.5">
        <div className="flex items-center justify-between">
          <label className="text-sm text-gray-600 font-medium">麦克风</label>
          <button
            onClick={refreshDevices}
            disabled={loadingDevices}
            className="text-xs text-blue-500 hover:text-blue-600 disabled:opacity-50 cursor-pointer"
          >
            {loadingDevices ? '刷新中…' : '刷新'}
          </button>
        </div>
        <Select
          label=""
          value={settings.selectedDevice}
          onChange={(v) => updateSettings({ selectedDevice: v })}
          options={[
            { value: '', label: '系统默认' },
            ...devices.map((d) => ({ value: d.id, label: d.label })),
          ]}
        />
        {devices.length === 0 && !loadingDevices && (
          <p className="text-xs text-gray-400">未检测到可用麦克风</p>
        )}
      </div>
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
  const showToast = useAppStore((s) => s.showToast);
  const [engineStatus, setEngineStatus] = useState<Record<string, any>>({});

  const currentEngine = ENGINES.find((e) => e.id === settings.engine) || ENGINES[0];

  const handleEngineChange = (engineId: string) => {
    updateSettings({ engine: engineId as AsrEngineType });
    // Automation: call switch_engine to auto-configure local servers
    invoke('switch_engine', { engine: engineId })
      .then((result) => {
        const status = result as any;
        setEngineStatus((prev) => ({ ...prev, [engineId]: status }));
        // Show feedback for local engine model check
        if (status.is_local) {
          if (status.status === 'ready') {
            showToast('SenseVoice 本地引擎已就绪');
          } else if (status.status === 'model_missing') {
            showToast('请先安装本地模型文件（见「本地资源」标签页）');
          }
        }
      })
      .catch((e) => console.error('switch_engine failed:', e));
  };

  // Check current engine status on mount
  useEffect(() => {
    invoke('switch_engine', { engine: settings.engine })
      .then((result) => {
        const status = result as any;
        setEngineStatus((prev) => ({ ...prev, [settings.engine]: status }));
      })
      .catch(() => {});
  }, []);

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
              <div className="flex items-center gap-2">
                <span className="text-sm font-medium text-gray-800 min-w-0 truncate">{engine.name}</span>
                {engine.isLocal && (
                  <span className="shrink-0 text-xs px-2 py-0.5 bg-green-100 text-green-700 rounded-full whitespace-nowrap">
                    本地离线
                  </span>
                )}
                {settings.engine === engine.id && (
                  <span className="text-blue-500 shrink-0 ml-auto">
                    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                      <polyline points="20 6 9 17 4 12"/>
                    </svg>
                  </span>
                )}
              </div>
              <p className="text-xs text-gray-500 mt-1 leading-relaxed">{engine.description}</p>
            </button>
          ))}
        </div>
      </div>

      {/* Engine-specific configuration */}
      <div className="border-t border-gray-200 pt-4">
        <div className="flex items-center gap-2 mb-3">
          <span className="text-sm font-medium text-gray-700 min-w-0 truncate">
            {currentEngine.name} 配置
          </span>
          {currentEngine.isLocal && (
            <span className="shrink-0 text-xs px-2 py-0.5 bg-green-100 text-green-700 rounded-full whitespace-nowrap">
              已内置
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
            {/* Local engine - live automation status */}
            {(() => {
              const status = engineStatus[settings.engine];
              const modelInstalled = status?.model_installed;
              const tokensInstalled = status?.tokens_installed;
              const vadInstalled = status?.vad_installed;
              const fullyReady = Boolean(modelInstalled && tokensInstalled && vadInstalled);

              return (
                <div className="flex flex-col gap-2">
                  {/* Model status */}
                  <div className={`p-3 rounded-lg border ${fullyReady ? 'bg-green-50 border-green-200' : 'bg-amber-50 border-amber-200'}`}>
                    <div className="flex items-center gap-2">
                      <span className={`w-2 h-2 rounded-full ${fullyReady ? 'bg-green-500' : 'bg-amber-500'}`} />
                      <span className={`text-xs font-medium ${fullyReady ? 'text-green-700' : 'text-amber-700'}`}>
                        {fullyReady ? '已就绪' : modelInstalled ? '缺少文件' : '未找到模型'}
                      </span>
                    </div>
                    {!modelInstalled && (
                      <p className="text-xs text-amber-600 mt-1">
                        模型已内置，若提示未找到请查看「本地资源」标签页。
                      </p>
                    )}
                  </div>

                  {/* Ready indicator — requires model + tokens + VAD */}
                  {fullyReady && (
                    <div className="p-3 rounded-lg border bg-green-50 border-green-200">
                      <div className="flex items-center gap-2">
                        <span className="w-2 h-2 rounded-full bg-green-500" />
                        <span className="text-xs font-medium text-green-700">可以开始使用</span>
                      </div>
                      <p className="text-xs text-gray-500 mt-1">
                        SenseVoice 本地引擎，无需网络连接
                      </p>
                    </div>
                  )}
                </div>
              );
            })()}

            {/* Local engines are fully self-configured: the model ships inside
                the app and paths are resolved at start_recording time. No
                address for the user to fill in. */}
          </div>
        )}
      </div>

      {/* Connection status indicator */}
      <div className="border-t border-gray-200 pt-4">
        {(() => {
          const localReady = Boolean(
            engineStatus[settings.engine]?.model_installed &&
              engineStatus[settings.engine]?.tokens_installed &&
              engineStatus[settings.engine]?.vad_installed
          );
          const ok = currentEngine.isLocal ? localReady : Boolean(settings.apiKey);
          return (
            <div className="flex items-start gap-2">
              <div className={`w-2 h-2 rounded-full mt-1 shrink-0 ${ok ? 'bg-green-500' : 'bg-amber-500'}`} />
              <span className="text-xs text-gray-500 leading-relaxed">
                {currentEngine.isLocal
                  ? ok
                    ? '本地引擎已就绪，按住快捷键即可开始说话'
                    : '本地资源缺失，请前往「本地资源」标签页查看'
                  : ok
                  ? '已配置，可以开始语音识别'
                  : '请填写 API Key 后使用'}
              </span>
            </div>
          );
        })()}
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
  const setAutoStart = useAppStore((s) => s.setAutoStart);

  return (
    <div className="flex flex-col gap-4">
      <Toggle
        label="开机自启"
        checked={settings.autoStart}
        onChange={(v) => setAutoStart(v)}
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
  const [checking, setChecking] = useState(false);

  const checkUpdate = async () => {
    setChecking(true);
    try {
      await openUrl('https://github.com/Aswellle/VoiceBoom');
    } catch {
      // Running in browser dev or plugin unavailable — fall back to a plain
      // window.open so the button still works.
      window.open('https://github.com/Aswellle/VoiceBoom', '_blank');
    } finally {
      setChecking(false);
    }
  };

  return (
    <div className="flex flex-col items-center gap-4 py-8">
      <div className="text-4xl">🎙️</div>
      <h2 className="text-xl font-semibold text-gray-800">VoiceBoom AI</h2>
      <p className="text-sm text-gray-500">版本 0.1.0 (MVP)</p>
      <p className="text-xs text-gray-400 text-center max-w-xs">
        实时流式智能语音输入法 — 像 Apple macOS 原生交互一样优雅，
        同时具备 AI 时代实时语音输入能力。
      </p>
      <button
        onClick={checkUpdate}
        disabled={checking}
        className="mt-2 px-4 py-1.5 text-xs rounded-full bg-blue-500 text-white hover:bg-blue-600 disabled:opacity-50 cursor-pointer transition-colors"
      >
        {checking ? '检查中…' : '检查更新'}
      </button>
      <div className="text-xs text-gray-400 mt-4 space-y-1 text-center">
        <p>React 19 + Tauri 2.0 + Rust</p>
        <p>OpenAI Whisper / Deepgram / SenseVoice</p>
        <p className="mt-3">MIT License (Non-Commercial)</p>
        <p>Copyright © 2026 Aswellle</p>
        <p className="text-[10px] text-gray-300 mt-2 max-w-[260px] leading-relaxed">
          本软件仅供非商业用途。商业使用需获得版权方书面授权。
        </p>
      </div>
    </div>
  );
}

/// Tab content: Local Resources management
function LocalResourcesTab() {
  const showToast = useAppStore((s) => s.showToast);
  const [resources, setResources] = useState<any[]>([]);
  const [busy, setBusy] = useState<Record<string, boolean>>({});

  // sherpa-onnx does in-process inference — no server process to start/stop,
  // so status only needs the resource file check (no port probing).
  const refreshStatus = () => {
    invoke('get_resource_status').then((status) => {
      setResources(status as any[]);
    }).catch(() => {});
  };

  useEffect(() => {
    refreshStatus();
    const interval = setInterval(refreshStatus, 3000); // Auto-refresh every 3s
    return () => clearInterval(interval);
  }, []);

  // Use a native file picker instead of prompt(): window.prompt is unreliable
  // inside the Tauri WebView and typing a full path by hand is error-prone.
  // Multi-select because FunASR needs two GGUF files (ASR model + FSMN VAD).
  const handleInstallModel = async (engine: string, engineName: string) => {
    try {
      const selected = await open({
        title: `选择 ${engineName} 模型文件（可多选）`,
        multiple: true,
        directory: false,
        filters: [{ name: '模型文件', extensions: ['onnx', 'txt', 'bin', 'gguf'] }],
      });
      const paths = Array.isArray(selected) ? selected : selected ? [selected] : [];
      if (paths.length === 0) return;

      setBusy((p) => ({ ...p, [engine]: true }));
      const res = (await invoke('install_model', { engine, modelPaths: paths })) as any;

      const count = res?.installed?.length ?? paths.length;
      if (res?.vad_required && !res?.vad_exists) {
        showToast(
          `已安装 ${count} 个文件，但还缺少 VAD 模型 ${res.vad_filename}，请一并选择安装`
        );
      } else if (!res?.model_exists) {
        showToast(`已复制 ${count} 个文件，但未识别到可用的识别模型，请确认选择的文件`);
      } else {
        showToast(`${engineName} 模型安装成功（${count} 个文件）`);
      }
    } catch (e) {
      showToast(`模型安装失败: ${e}`);
    } finally {
      setBusy((p) => ({ ...p, [engine]: false }));
      refreshStatus();
    }
  };

  const formatSize = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
  };

  const engines = [
    { id: 'sensevoice', name: 'SenseVoice', description: '阿里达摩院多语言语音识别，本地离线运行，中文识别优秀', modelUrl: 'https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17' },
  ];

  return (
    <div className="flex flex-col gap-4">
      <p className="text-sm text-gray-500 leading-relaxed">
        查看本地语音识别状态。模型文件已内置，打开即用。
      </p>

      {engines.map((engine) => {
        const resource = resources.find((r) => r.engine === engine.id);
        const isReady = resource?.is_ready;
        const modelExists = resource?.model_file_exists;
        const tokensExists = resource?.tokens_file_exists;
        const vadExists = resource?.vad_model_exists;

        return (
          <div key={engine.id} className="p-4 rounded-lg border border-gray-200 bg-white">
            <div className="flex items-start justify-between gap-3">
              <div className="min-w-0">
                <h3 className="text-sm font-medium text-gray-800">{engine.name}</h3>
                <p className="text-xs text-gray-500 mt-0.5">{engine.description}</p>
              </div>
              {isReady ? (
                <span className="shrink-0 text-xs px-2 py-1 bg-green-100 text-green-700 rounded-full whitespace-nowrap">已就绪</span>
              ) : modelExists ? (
                <span className="shrink-0 text-xs px-2 py-1 bg-amber-100 text-amber-700 rounded-full whitespace-nowrap">缺少文件</span>
              ) : (
                <span className="shrink-0 text-xs px-2 py-1 bg-gray-100 text-gray-500 rounded-full whitespace-nowrap">未安装</span>
              )}
            </div>

            {/* Status details */}
            {resource && (
              <div className="mt-3 pt-3 border-t border-gray-100">
                <div className="grid grid-cols-2 gap-2 text-xs">
                  <div className="flex items-center gap-1">
                    <span className={`w-2 h-2 rounded-full shrink-0 ${modelExists ? 'bg-green-500' : 'bg-gray-300'}`} />
                    <span className="text-gray-500">语音识别</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className={`w-2 h-2 rounded-full shrink-0 ${tokensExists ? 'bg-green-500' : 'bg-gray-300'}`} />
                    <span className="text-gray-500">语言词库</span>
                  </div>
                  <div className="flex items-center gap-1">
                    <span className={`w-2 h-2 rounded-full shrink-0 ${vadExists ? 'bg-green-500' : 'bg-gray-300'}`} />
                    <span className="text-gray-500">语音检测</span>
                  </div>
                  <div className="text-gray-500">
                    大小: <span className="text-gray-700">{formatSize(resource.size_bytes)}</span>
                  </div>
                </div>
                {resource && resource.is_bundled && (
                  <p className="text-xs text-gray-400 mt-2">
                    模型文件已随应用内置，无需额外下载
                  </p>
                )}
                {resource && !resource.is_bundled && (
                  <p className="text-xs text-gray-400 mt-2">
                    使用自定义模型文件
                  </p>
                )}
              </div>
            )}

            {/* Guidance when model files are missing or incomplete */}
            {!isReady && (
              <div className="mt-3 p-3 bg-amber-50 rounded-lg border border-amber-200">
                <p className="text-xs text-amber-800 font-medium">
                  {modelExists ? '文件不完整' : '未找到模型文件'}
                </p>
                <p className="text-xs text-amber-600 mt-1 leading-relaxed">
                  需要三个文件：<b>{resource?.default_model_filename}</b>（识别模型）、
                  <b>{resource?.tokens_filename}</b>（词表）和
                  <b>{resource?.vad_filename}</b>（语音检测）。下方按钮支持一次多选。
                </p>
                <div className="mt-2 flex flex-wrap gap-2">
                  <button
                    onClick={() => handleInstallModel(engine.id, engine.name)}
                    disabled={busy[engine.id]}
                    className="px-3 py-1.5 bg-blue-500 text-white text-xs rounded-lg hover:bg-blue-600 transition-colors disabled:opacity-50 disabled:cursor-not-allowed whitespace-nowrap"
                  >
                    {busy[engine.id] ? '安装中…' : '选择文件安装'}
                  </button>
                  <a
                    href={engine.modelUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-block px-3 py-1.5 bg-amber-500 text-white text-xs rounded-lg hover:bg-amber-600 transition-colors whitespace-nowrap"
                  >
                    下载模型
                  </a>
                </div>
                <p className="text-xs text-amber-500 mt-2 leading-relaxed">
                  也可以把下载的文件放到应用程序同目录的 models 文件夹，重启后自动加载。
                </p>
              </div>
            )}
          </div>
        );
      })}

      <div className="p-3 bg-blue-50 rounded-lg border border-blue-200">
        <p className="text-xs text-blue-600 leading-relaxed">
          💡 模型文件已内置，正常情况下无需额外操作。此页面用于查看状态或安装自定义模型。
        </p>
      </div>
    </div>
  );
}

/// Main settings panel
export function SettingsPanel() {
  const [activeTab, setActiveTab] = useState<TabId>('local'); // Default to local tab so users see bundled model status first
  // The settings window is a separate WebView with its own store instance, so it
  // needs its own toast surface — showToast calls here were previously invisible.
  const toastMessage = useAppStore((s) => s.toastMessage);

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
    <div className="relative flex h-full overflow-hidden bg-gray-50">
      {/* Sidebar tabs — shrink-0 so it never collapses and push content out */}
      <nav className="w-36 shrink-0 overflow-y-auto bg-white border-r border-gray-200 py-4">
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

      {/* Tab content — min-w-0 lets flex children shrink so long strings wrap
          inside the panel instead of pushing content past the viewport */}
      <main className="flex-1 min-w-0 p-5 overflow-y-auto overflow-x-hidden">
        <motion.div
          key={activeTab}
          initial={{ opacity: 0, y: 5 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.15 }}
          className="min-w-0 break-words"
        >
          {renderTab()}
        </motion.div>
      </main>

      {/* Toast surface for this window */}
      {toastMessage && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0 }}
          className="absolute bottom-5 left-1/2 -translate-x-1/2 max-w-[calc(100%-3rem)] px-4 py-2 bg-gray-800/95 text-white text-xs rounded-2xl shadow-lg z-50 text-center leading-relaxed break-words"
        >
          {toastMessage}
        </motion.div>
      )}
    </div>
  );
}
