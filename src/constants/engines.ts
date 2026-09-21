// Shared engine metadata — single source of truth for display names and capabilities.
// Used by both FloatingWindow (HUD label) and Settings (AI tab engine selection).
// P0-2: Engine IDs now align with backend ProviderId serde values.

import type { AsrEngineType } from '../stores/useAppStore';

export interface EngineInfo {
  id: AsrEngineType;
  name: string;
  description: string;
  isLocal: boolean;
  keyPlaceholder?: string;
  keyHelp?: string;
  endpointPlaceholder?: string;
  downloadUrl?: string;
  downloadHelp?: string;
  /// P0-3: Experimental engines are hidden from the main selection UI.
  experimental?: boolean;
}

export const ENGINES: EngineInfo[] = [
  {
    id: 'local_sense_voice',
    name: 'SenseVoice',
    description: '阿里达摩院多语言引擎，内置离线运行，中文识别优秀',
    isLocal: true,
    keyPlaceholder: '（本地引擎无需 API Key）',
    keyHelp: '本地引擎不需要 API Key',
    endpointPlaceholder: '（自动配置）',
    downloadUrl: 'https://huggingface.co/csukuangfj/sherpa-onnx-sense-voice-zh-en-ja-ko-yue-2024-07-17',
    downloadHelp: '模型已内置，开箱即用',
  },
  {
    id: 'openai_realtime',
    name: 'OpenAI Whisper API',
    description: 'OpenAI 官方云端语音识别，支持多语言，准确率高',
    isLocal: false,
    keyPlaceholder: 'sk-xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 platform.openai.com/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.openai.com/v1/audio/transcriptions',
  },
  {
    id: 'deepgram_streaming',
    name: 'Deepgram',
    description: '专业语音识别服务，低延迟流式转写',
    isLocal: false,
    keyPlaceholder: 'xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 console.deepgram.com/settings/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.deepgram.com/v1/listen',
  },
  {
    id: 'openai_whisper',
    name: 'OpenAI Whisper (REST)',
    description: 'OpenAI Whisper REST API，适合非实时场景',
    isLocal: false,
    keyPlaceholder: 'sk-xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 platform.openai.com/api-keys 获取 API Key',
    endpointPlaceholder: 'https://api.openai.com/v1/audio/transcriptions',
  },
  {
    id: 'custom_openai_compatible',
    name: 'Custom OpenAI-Compatible',
    description: '兼容 OpenAI API 格式的自定义服务（Ollama、本地服务等）',
    isLocal: false,
    keyPlaceholder: 'sk-xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '输入你的 API Key',
    endpointPlaceholder: 'wss://your-server.com/v1/audio/transcriptions',
  },
  {
    // P0-3: Whisper.cpp is not yet implemented — hidden from main selection.
    // Cast via unknown because whisper_cpp was removed from AsrEngineType.
    id: 'whisper_cpp' as unknown as AsrEngineType,
    name: 'Whisper.cpp',
    description: '多语言离线引擎，本地运行（实验性，尚未实现）',
    isLocal: true,
    experimental: true,
    keyPlaceholder: '（本地引擎无需 API Key）',
    keyHelp: '本地引擎不需要 API Key',
    endpointPlaceholder: '（暂不可用）',
    downloadUrl: 'https://github.com/ggerganov/whisper.cpp',
    downloadHelp: '当前版本使用 SenseVoice，Whisper 支持将在后续版本加入',
  },
];



/** P0-3: Experimental engines not yet implemented — shown as disabled in UI. */
export const EXPERIMENTAL_ENGINES: EngineInfo[] = ENGINES.filter((e) => e.experimental);

export const ENGINE_DISPLAY_NAMES: Record<AsrEngineType, string> = {
  local_sense_voice: 'SenseVoice',
  openai_realtime: 'Whisper API',
  deepgram_streaming: 'Deepgram',
  openai_whisper: 'Whisper REST',
  custom_openai_compatible: 'Custom',
};

/// P0-3: Filter out experimental engines for the main selection UI.
export const AVAILABLE_ENGINES: EngineInfo[] = ENGINES.filter((e) => !e.experimental);

export const getEngineInfo = (id: AsrEngineType): EngineInfo | undefined =>
  ENGINES.find((e) => e.id === id);
