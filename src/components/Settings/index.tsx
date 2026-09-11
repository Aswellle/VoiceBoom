// Settings panel — configuration UI for VoiceBoom
// P1: Restructured from 7 engineering tabs to 5 task-oriented sections

import { useState, useEffect, useCallback } from 'react';
import { motion } from 'framer-motion';
import { useAppStore, type AsrEngineType } from '../../stores/useAppStore';
import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { open as openUrl } from '@tauri-apps/plugin-shell';
import { ENGINES, type EngineInfo } from '../../constants/engines';

type TabId = 'basic' | 'ai' | 'model' | 'appearance' | 'personalization' | 'advanced';

interface Tab {
  id: TabId;
  label: string;
  icon: string;
}

const TABS: Tab[] = [
  { id: 'basic', label: '基本', icon: '🎤' },
  { id: 'ai', label: 'AI', icon: '🤖' },
  { id: 'model', label: '模型', icon: '📦' },
  { id: 'appearance', label: '外观', icon: '🎨' },
  { id: 'personalization', label: '个性化', icon: '✨' },
  { id: 'advanced', label: '高级', icon: '⚙️' },
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
/// Select component — accessible dropdown
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
      {label && <label className="text-sm" style={{ color: 'var(--text-primary)' }}>{label}</label>}
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2"
        style={{
          borderColor: 'var(--border-subtle)',
          background: 'var(--surface-base)',
          color: 'var(--text-primary)',
        }}
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

/// Text input component — accessible input with proper labeling
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
      <label className="text-sm" style={{ color: 'var(--text-primary)' }}>{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className={`rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2 ${
          disabled ? 'opacity-50 cursor-not-allowed' : ''
        }`}
        style={{
          borderColor: 'var(--border-subtle)',
          background: disabled ? 'var(--surface-muted)' : 'var(--surface-base)',
          color: 'var(--text-primary)',
        }}
      />
      {helpText && <p className="text-xs mt-0.5" style={{ color: 'var(--text-tertiary)' }}>{helpText}</p>}
    </div>
  );
}
/// Toggle component
/// Toggle component — accessible switch with proper ARIA semantics
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
    <label className="flex items-center justify-between gap-3 cursor-pointer py-2">
      <span className="text-sm min-w-0" style={{ color: 'var(--text-primary)' }}>{label}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`
          relative w-11 h-6 shrink-0 rounded-full transition-colors
          focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2
          ${checked ? 'bg-blue-500' : 'bg-gray-300'}
        `}
        style={{
          backgroundColor: checked ? 'var(--accent)' : 'var(--border-subtle)',
        }}
      >
        <span
          className={`
            absolute top-1 w-4 h-4 rounded-full bg-white shadow transition-transform
            ${checked ? 'translate-x-6' : 'translate-x-1'}
          `}
        />
      </button>
    </label>
  );
}

/// Tab content: Basic settings — what users care about most
function BasicTab() {
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
          <label className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>麦克风</label>
          <button
            onClick={refreshDevices}
            disabled={loadingDevices}
            className="text-xs cursor-pointer"
            style={{ color: 'var(--accent)' }}
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
          <p className="text-xs" style={{ color: 'var(--text-tertiary)' }}>未检测到可用麦克风</p>
        )}
      </div>
      <Slider
        label="VAD 灵敏度"
        value={settings.vadSensitivity}
        min={0}
        max={100}
        onChange={(v) => updateSettings({ vadSensitivity: v })}
      />
      <Select
        label="上屏模式"
        value={settings.inputPolicy}
        onChange={(v) => updateSettings({ inputPolicy: v as 'direct' | 'confirm' | 'recognize' })}
        options={[
          { value: 'direct', label: '直接上屏（识别后立即输入）' },
          { value: 'confirm', label: '确认上屏（显示插入按钮）' },
          { value: 'recognize', label: '仅识别（不输入到其他应用）' },
        ]}
      />
      <Select
        label="HUD 显示密度"
        value={settings.hudDensity}
        onChange={(v) => updateSettings({ hudDensity: v as 'compact' | 'standard' | 'expanded' })}
        options={[
          { value: 'compact', label: '紧凑（仅当前句子）' },
          { value: 'standard', label: '标准（3-5 句）' },
          { value: 'expanded', label: '展开（完整记录）' },
        ]}
      />
      <TextInput
        label="录音快捷键"
        value={settings.shortcut}
        onChange={(v) => updateSettings({ shortcut: v })}
        placeholder="例如: Ctrl+Space"
      />
      <p className="text-xs" style={{ color: 'var(--text-tertiary)' }}>
        按住快捷键开始录音，松开停止。支持 Ctrl、Alt、Shift、Cmd 等修饰键组合。
      </p>
    </div>
  );
}
/// Tab content: AI settings — engine selection and configuration
function AITab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const showToast = useAppStore((s) => s.showToast);
  const providers = useAppStore((s) => s.providers);
  const loadProviders = useAppStore((s) => s.loadProviders);
  const resolveProvider = useAppStore((s) => s.resolveProvider);
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
    loadProviders();
  }, []);

  const handleModeChange = async (mode: 'automatic' | 'offline' | 'cloud') => {
    try {
      await resolveProvider(mode, true);
      showToast(`语音引擎已切换为：${mode === 'automatic' ? '自动' : mode === 'offline' ? '离线' : '云端'}`);
    } catch (e) {
      console.error('handleModeChange failed:', e);
    }
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

        {/* Voice Engine Mode (Phase 3: auto-fallback) */}
        <div className="flex flex-col gap-2">
          <label className="text-sm text-gray-600 font-medium">语音引擎模式</label>
          <div className="grid grid-cols-3 gap-2">
            {([
              { id: 'automatic', label: '自动', desc: '优先本地，智能回退' },
              { id: 'offline', label: '离线', desc: '仅本地引擎' },
              { id: 'cloud', label: '云端', desc: '使用云端 ASR' },
            ] as const).map((mode) => (
              <button
                key={mode.id}
                onClick={() => handleModeChange(mode.id)}
                className="text-left p-2 rounded-lg border-2 transition-all"
                style={{
                  borderColor: 'var(--border-subtle)',
                  background: 'var(--surface-base)',
                }}
              >
                <span className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>
                  {mode.label}
                </span>
                <p className="text-[10px] mt-0.5" style={{ color: 'var(--text-tertiary)' }}>
                  {mode.desc}
                </p>
              </button>
            ))}
          </div>
          <p className="text-xs" style={{ color: 'var(--text-tertiary)' }}>
            自动模式下，本地引擎不可用时将智能回退到云端。
          </p>
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
                      <p className="text-xs mt-1" style={{ color: 'var(--text-tertiary)' }}>
                        模型已内置，若提示未找到请重新安装模型文件。
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
              <span className="text-xs leading-relaxed" style={{ color: 'var(--text-secondary)' }}>
                {currentEngine.isLocal
                  ? ok
                    ? '本地引擎已就绪，按住快捷键即可开始说话'
                    : '本地资源缺失，请重新安装模型文件'
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

/// Format bytes into a human-readable size string.
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B';
  const units = ['B', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const value = bytes / Math.pow(1024, i);
  return `${value.toFixed(i === 0 ? 0 : 1)} ${units[i]}`;
}

/// Tab content: Model management — download, install, and manage local ASR models.
function ModelTab() {
  const models = useAppStore((s) => s.models);
  const loadModels = useAppStore((s) => s.loadModels);
  const downloadModel = useAppStore((s) => s.downloadModel);
  const cancelModelDownload = useAppStore((s) => s.cancelModelDownload);
  const deleteModelVersion = useAppStore((s) => s.deleteModelVersion);
  const setActiveModel = useAppStore((s) => s.setActiveModel);

  useEffect(() => {
    loadModels();
  }, [loadModels]);

  return (
    <div className="flex flex-col gap-4">
      <p className="text-xs" style={{ color: 'var(--text-tertiary)' }}>
        本地 ASR 模型独立于应用管理。首次使用本地引擎时下载模型，后续可独立更新。
      </p>
      {models.length === 0 ? (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>暂无可用模型</p>
      ) : (
        models.map((model) => {
          const isDownloading = model.state === 'downloading';
          const isReady = model.state === 'ready';
          const progress = model.progress ?? 0;
          return (
            <div
              key={model.id}
              className="p-3 rounded-lg border"
              style={{ borderColor: 'var(--border-subtle)', background: 'var(--surface-base)' }}
            >
              <div className="flex items-center justify-between mb-2">
                <div>
                  <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                    {model.id}
                  </span>
                  <span className="ml-2 text-xs" style={{ color: 'var(--text-tertiary)' }}>
                    v{model.version} · {formatBytes(model.size_bytes)}
                  </span>
                </div>
                <span className="text-xs" style={{ color: 'var(--text-tertiary)' }}>
                  {model.languages.join(', ')}
                </span>
              </div>

              {isDownloading && (
                <div className="mb-2">
                  <div className="h-1.5 rounded-full overflow-hidden" style={{ background: 'var(--border-subtle)' }}>
                    <div
                      className="h-full rounded-full transition-all"
                      style={{ width: `${Math.round(progress * 100)}%`, background: 'var(--accent)' }}
                    />
                  </div>
                  <p className="text-xs mt-1" style={{ color: 'var(--text-tertiary)' }}>
                    下载中… {Math.round(progress * 100)}%
                  </p>
                </div>
              )}

              {model.error && (
                <p className="text-xs mb-2" style={{ color: 'var(--danger, #e55)' }}>{model.error}</p>
              )}

              <div className="flex gap-2">
                {isDownloading ? (
                  <button
                    onClick={() => cancelModelDownload(model.id)}
                    className="text-xs px-3 py-1 rounded-md cursor-pointer"
                    style={{ background: 'var(--danger, #e55)', color: '#fff' }}
                  >
                    取消
                  </button>
                ) : isReady ? (
                  <span className="text-xs px-3 py-1 rounded-md" style={{ background: 'var(--accent-muted)', color: 'var(--accent)' }}>
                    ✓ 已就绪
                  </span>
                ) : (
                  <button
                    onClick={() => downloadModel(model.id)}
                    className="text-xs px-3 py-1 rounded-md cursor-pointer"
                    style={{ background: 'var(--accent)', color: '#fff' }}
                  >
                    下载
                  </button>
                )}
                {model.installed_versions.length > 0 && (
                  <button
                    onClick={() => deleteModelVersion(model.engine, model.version)}
                    className="text-xs px-3 py-1 rounded-md cursor-pointer"
                    style={{ border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
                  >
                    删除
                  </button>
                )}
                {model.installed_versions.includes(model.version) && !isReady && (
                  <button
                    onClick={() => setActiveModel(model.engine, model.version)}
                    className="text-xs px-3 py-1 rounded-md cursor-pointer"
                    style={{ border: '1px solid var(--border-subtle)', color: 'var(--text-secondary)' }}
                  >
                    激活
                  </button>
                )}
              </div>
            </div>
          );
        })
      )}
    </div>
  );
}

/// Tab content: Appearance settings — HUD style, theme, display
function AppearanceTab() {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  return (
    <div className="flex flex-col gap-4">
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

/// Tab content: Personalization settings — future home for dictionary, modes
function PersonalizationTab() {
  return (
    <div className="flex flex-col gap-4">
      <div className="p-4 rounded-lg" style={{ background: 'var(--surface-muted)' }}>
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
          个性化功能将在后续版本中加入：
        </p>
        <ul className="mt-2 text-xs space-y-1" style={{ color: 'var(--text-tertiary)' }}>
          <li>• 个人词典</li>
          <li>• 常用短语</li>
          <li>• 语气/格式模式</li>
          <li>• 应用规则</li>
        </ul>
      </div>
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
      <div className="text-xs mt-2 space-y-1" style={{ color: 'var(--text-tertiary)' }}>
        <p>数据存储位置: %APPDATA%\com.voiceboom.app\</p>
        <p>日志级别: INFO</p>
      </div>
    </div>
  );
}

/// Main settings panel
export function SettingsPanel() {
  const [activeTab, setActiveTab] = useState<TabId>('basic'); // P1: Default to basic tab
  // The settings window is a separate WebView with its own store instance, so it
  // needs its own toast surface — showToast calls here were previously invisible.
  const toastMessage = useAppStore((s) => s.toastMessage);

  const renderTab = () => {
    switch (activeTab) {
      case 'basic':
        return <BasicTab />;
      case 'ai':
        return <AITab />;
      case 'model':
        return <ModelTab />;
      case 'appearance':
        return <AppearanceTab />;
      case 'personalization':
        return <PersonalizationTab />;
      case 'advanced':
        return <AdvancedTab />;
    }
  };

  return (
    <div className="relative flex h-full overflow-hidden" style={{ background: 'var(--surface-muted)' }}>
      {/* Sidebar tabs — shrink-0 so it never collapses and push content out */}
      <nav className="w-40 shrink-0 overflow-y-auto py-4" style={{ background: 'var(--surface-base)', borderRight: '1px solid var(--border-subtle)' }}>
        {TABS.map((tab) => (
          <button
            key={tab.id}
            onClick={() => setActiveTab(tab.id)}
            className={`
              w-full text-left px-4 py-2.5 text-sm transition-colors
              ${
                activeTab === tab.id
                  ? 'font-medium'
                  : 'hover:opacity-80'
              }
            `}
            style={{
              background: activeTab === tab.id ? 'var(--accent-muted)' : 'transparent',
              color: activeTab === tab.id ? 'var(--accent)' : 'var(--text-secondary)',
            }}
          >
            <span className="mr-2">{tab.icon}</span>
            {tab.label}
          </button>
        ))}
        {/* P1: About as footer instead of tab */}
        <div className="mt-auto pt-4 px-4">
          <p className="text-[10px]" style={{ color: 'var(--text-tertiary)' }}>
            VoiceBoom AI v0.1.0
          </p>
          <button
            onClick={() => openUrl('https://github.com/Aswellle/VoiceBoom').catch(() => window.open('https://github.com/Aswellle/VoiceBoom', '_blank'))}
            className="text-[10px] mt-1 cursor-pointer"
            style={{ color: 'var(--accent)' }}
          >
            检查更新
          </button>
        </div>
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
          className="absolute bottom-5 left-1/2 -translate-x-1/2 max-w-[calc(100%-3rem)] px-4 py-2 rounded-2xl shadow-lg z-50 text-center leading-relaxed break-words"
          style={{ background: 'var(--surface-base)', color: 'var(--text-primary)', fontSize: '12px' }}
        >
          {toastMessage}
        </motion.div>
      )}
    </div>
  );
}
