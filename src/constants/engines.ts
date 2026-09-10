// Shared engine metadata — single source of truth for display names and capabilities.
// Used by both FloatingWindow (HUD label) and Settings (AI tab engine selection).

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
}

export const ENGINES: EngineInfo[] = [
  {
    id: 'funasr',
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
    id: 'openai_whisper',
    name: 'OpenAI Whisper API',
    description: 'OpenAI 官方云端语音识别，支持多语言，准确率高',
    isLocal: false,
    keyPlaceholder: 'sk-xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 platform.openai.com/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.openai.com/v1/audio/transcriptions',
  },
  {
    id: 'deepgram',
    name: 'Deepgram',
    description: '专业语音识别服务，低延迟流式转写',
    isLocal: false,
    keyPlaceholder: 'xxxxxxxxxxxxxxxxxxxxxxxx',
    keyHelp: '从 console.deepgram.com/settings/api-keys 获取 API Key',
    endpointPlaceholder: 'wss://api.deepgram.com/v1/listen',
  },
  {
    id: 'whisper_cpp',
    name: 'Whisper.cpp',
    description: '多语言离线引擎，本地运行（后续版本整合）',
    isLocal: true,
    keyPlaceholder: '（本地引擎无需 API Key）',
    keyHelp: '本地引擎不需要 API Key',
    endpointPlaceholder: '（暂不可用）',
    downloadUrl: 'https://github.com/ggerganov/whisper.cpp',
    downloadHelp: '当前版本使用 SenseVoice，Whisper 支持将在后续版本加入',
  },
];

export const ENGINE_DISPLAY_NAMES: Record<AsrEngineType, string> = {
  funasr: 'SenseVoice',
  openai_whisper: 'Whisper API',
  deepgram: 'Deepgram',
  whisper_cpp: 'Whisper.cpp',
};

export const getEngineInfo = (id: AsrEngineType): EngineInfo | undefined =>
  ENGINES.find((e) => e.id === id);
