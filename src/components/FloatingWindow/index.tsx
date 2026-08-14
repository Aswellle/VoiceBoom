// FloatingWindow — the main voice recognition display
// Shows real-time recognition results with glassmorphism styling
// Features: newest-first layout, text trimming, animations, drag support

import { useCallback, useEffect, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore, RecognitionSegment } from '../../stores/useAppStore';
import { Waveform } from '../Waveform';
import { useGlobalShortcut } from '../../hooks/useGlobalShortcut';
import { useAsr } from '../../hooks/useAsr';

/// Animation variants for text segments
const segmentVariants = {
  initial: { opacity: 0, x: 10, scale: 0.95 },
  animate: { opacity: 1, x: 0, scale: 1 },
  exit: { opacity: 0, x: -20, scale: 0.9 },
};

const containerVariants = {
  initial: { opacity: 0, scale: 0.8 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.8 },
};

/// Single recognition segment display
function SegmentItem({
  segment,
  isNewest,
  reduceMotion,
  fontSize,
}: {
  segment: RecognitionSegment;
  isNewest: boolean;
  reduceMotion: boolean;
  fontSize: number;
}) {
  const showToast = useAppStore((s) => s.showToast);
  const handleClick = useCallback(() => {
    // Copy to clipboard on click — m7 fix: visual feedback
    navigator.clipboard.writeText(segment.text).then(() => {
      showToast('已复制');
    }).catch(() => {
      // Fallback: use Tauri clipboard or ignore
    });
  }, [segment.text, showToast]);

  return (
    <motion.div
      layout
      variants={segmentVariants}
      initial={reduceMotion ? false : 'initial'}
      animate="animate"
      exit="exit"
      transition={{ duration: 0.2, ease: 'easeOut' }}
      onClick={handleClick}
      className={`
        cursor-pointer select-all rounded-lg px-3 py-1.5
        transition-colors duration-150
        ${isNewest ? 'opacity-100' : 'opacity-35'}
        hover:bg-white/10
      `}
      style={{ fontSize }}
    >
      <span className="text-gray-800 dark:text-gray-100 leading-snug">
        {segment.text}
      </span>
      {segment.language && (
        <span className="ml-2 text-[10px] text-gray-400 uppercase">
          {segment.language === 'zh' ? '中' : segment.language}
        </span>
      )}
    </motion.div>
  );
}

/// Main floating window component
export function FloatingWindow() {
  // Single useAsr instance shared by both the global-hotkey path and the manual
  // start/stop button, so they agree on the isListeningRef guard (prevents the
  // stuck-recording / swallowed-hotkey desync).
  const { startListening, stopListening } = useAsr();
  useGlobalShortcut(startListening, stopListening);

  const status = useAppStore((s) => s.status);
  const segments = useAppStore((s) => s.segments);
  const currentPartial = useAppStore((s) => s.currentPartial);
  const settings = useAppStore((s) => s.settings);
  const loadSettings = useAppStore((s) => s.loadSettings);
  const toastMessage = useAppStore((s) => s.toastMessage);
  const settingsLoaded = useAppStore((s) => s.settingsLoaded);
  const shortcutRegistered = useAppStore((s) => s.shortcutRegistered);

  // Engine readiness check — populated on mount and when engine switches.
  // For local engines, checks if model files exist; for cloud engines,
  // reports whether an API key is configured. Used to show "configure first"
  // hints instead of "press shortcut" when the selected engine can't work.
  const [engineReady, setEngineReady] = useState<boolean | null>(null);

  // Check engine status on mount and when settings.engine changes. Skip until
  // initial settings are loaded so we don't probe the default engine before
  // loadSettings resolves (would show a transiently wrong readiness hint).
  useEffect(() => {
    if (!settingsLoaded) return;
    invoke('switch_engine', { engine: settings.engine })
      .then((result: any) => {
        // Local engines report model_installed; cloud engines don't.
        // For local, ready = all models present. For cloud, ready = has key.
        const isLocal = result.is_local;
        const ready = isLocal
          ? (result.model_installed && result.tokens_installed && result.vad_installed)
          : Boolean(settings.apiKey);
        setEngineReady(ready);
      })
      .catch(() => setEngineReady(false));
  }, [settings.engine, settings.apiKey, settingsLoaded]);

  // Reload settings when the engine is switched so the label stays current
  useEffect(() => {
    const unlisten = listen('engine:switched', () => {
      loadSettings().catch(console.error);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadSettings]);

  // Audio flow diagnostics — heartbeat from the bridge task tells us whether
  // microphone samples are actually reaching the ASR pipeline.
  // Diagnostic state — visible in the UI so we can see what's happening
  const [diag, setDiag] = useState({
    shortcutPressed: false,
    audioFrames: 0,
    lastPartial: '',
    lastFinal: '',
    error: '',
  });

  // Listen for shortcut events
  useEffect(() => {
    const unPressed = listen('shortcut:pressed', () => {
      setDiag((d) => ({ ...d, shortcutPressed: true }));
    });
    const unReleased = listen('shortcut:released', () => {
      setDiag((d) => ({ ...d, shortcutPressed: false }));
    });
    return () => {
      unPressed.then((f) => f());
      unReleased.then((f) => f());
    };
  }, []);

  // Listen for ASR events
  useEffect(() => {
    const unHeartbeat = listen<{ frames: number }>('asr:heartbeat', (e) => {
      setDiag((d) => ({ ...d, audioFrames: e.payload.frames }));
    });
    const unError = listen<string>('asr:error', (e) => {
      setDiag((d) => ({ ...d, error: e.payload }));
    });
    const unResult = listen<{ text: string; is_final: boolean }>('asr:result', (e) => {
      if (e.payload.is_final) {
        setDiag((d) => ({ ...d, lastFinal: e.payload.text, lastPartial: '' }));
      } else {
        setDiag((d) => ({ ...d, lastPartial: e.payload.text }));
      }
    });
    return () => {
      unHeartbeat.then((f) => f());
      unError.then((f) => f());
      unResult.then((f) => f());
    };
  }, []);

  const isListening = status === 'listening';
  const hasContent = segments.length > 0 || currentPartial;

  // Status text with engine readiness awareness
  const statusText = isListening
    ? (diag.audioFrames > 0 ? `正在聆听... (${diag.audioFrames} 帧)` : '正在聆听...')
    : hasContent
    ? ''
    : engineReady === false
    ? '⚙️ 请先打开设置配置引擎'
    : engineReady === null
    ? '检查引擎状态...'
    : '按住快捷键说话';

  // Engine display label
  const engineLabel: Record<string, string> = {
    openai_whisper: 'Whisper API',
    deepgram: 'Deepgram',
    whisper_cpp: 'SenseVoice', // Legacy ID, now maps to the same backend
    funasr: 'SenseVoice',
  };
  const currentEngineLabel = engineLabel[settings.engine] || settings.engine;

  return (
    <motion.div
      variants={containerVariants}
      initial={settings.reduceMotion ? false : 'initial'}
      animate="animate"
      exit="exit"
      transition={{ type: 'spring', stiffness: 300, damping: 25 }}
      className={`
        glass w-full h-full flex flex-col
        ${isListening ? 'ring-2 ring-red-400/30' : ''}
        ${settings.theme === 'dark' ? 'glass-dark' : ''}
      `}
      style={{
        opacity: settings.opacity,
        background:
          settings.theme === 'dark'
            ? 'rgba(0, 0, 0, 0.65)'
            : 'rgba(255, 255, 255, 0.72)',
      }}
    >
      {/* Header: status indicator + waveform — M13 fix: drag handle for window repositioning */}
      <div
        className="flex items-center gap-3 px-4 pt-3 pb-1 cursor-move"
        onMouseDown={() => {
          getCurrentWebviewWindow().startDragging().catch(() => {});
        }}
      >
        {/* Recording indicator */}
        <div className="flex items-center gap-2">
          <div
            className={`
              w-2.5 h-2.5 rounded-full
              ${isListening ? 'bg-red-500 recording-dot' : 'bg-gray-300'}
            `}
          />
          {isListening && <Waveform />}
        </div>

        {/* Status text */}
        {statusText && (
          <span className="text-xs text-gray-400 italic flex-1">{statusText}</span>
        )}

        {/* Diagnostic indicator — shows shortcut/audio/ASR state at a glance */}
        <div className="flex items-center gap-1.5 text-[10px]" title="诊断: 快捷键/音频/识别状态">
          <span className={`w-1.5 h-1.5 rounded-full ${shortcutRegistered ? 'bg-green-400' : 'bg-red-400'}`} title={shortcutRegistered ? '快捷键已注册' : '快捷键未注册'} />
          <span className={`w-1.5 h-1.5 rounded-full ${diag.shortcutPressed ? 'bg-red-500 animate-pulse' : 'bg-gray-400'}`} title={diag.shortcutPressed ? '快捷键按下' : '快捷键松开'} />
          <span className={`w-1.5 h-1.5 rounded-full ${diag.audioFrames > 0 ? 'bg-blue-400' : 'bg-gray-400'}`} title={`音频帧: ${diag.audioFrames}`} />
          {diag.error && <span className="w-1.5 h-1.5 rounded-full bg-red-600" title={diag.error} />}
        </div>

        {/* Current engine indicator */}
        <span className="text-[10px] px-2 py-0.5 bg-white/30 rounded-full text-gray-500 whitespace-nowrap" title={`当前引擎: ${currentEngineLabel}`}>
          {currentEngineLabel}
        </span>

        {/* Manual start button — directly invokes Tauri commands to test the core pipeline */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => {
            if (isListening) {
              stopListening();
            } else {
              startListening();
            }
          }}
          className={`px-2 py-0.5 rounded-full text-[10px] font-medium transition-colors cursor-pointer ${
            isListening
              ? 'bg-red-500 text-white hover:bg-red-600'
              : 'bg-green-500/80 text-white hover:bg-green-600'
          }`}
          title="点击开始/停止录音"
        >
          {isListening ? '停止' : '说话'}
        </button>

        {/* Settings button */}
        <button
          onMouseDown={(e) => {
            // Stop propagation so the parent drag handle doesn't start dragging.
            // Deliberately no preventDefault(): that suppressed the click event
            // itself, so the gear stopped opening settings.
            e.stopPropagation();
          }}
          onClick={() => invoke('open_settings').catch((err) => console.error('open_settings failed:', err))}
          className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-white/20 transition-colors text-gray-400 hover:text-gray-600 cursor-pointer"
          title="设置"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68 1.65 1.65 0 0 0 10 3.17V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
      </div>

      {/* Diagnostic text — shows exactly what's happening */}
      <div className="px-4 pb-1">
        <div className="text-[10px] text-gray-400 font-mono leading-tight">
          {diag.error ? (
            <span className="text-red-400">❌ {diag.error}</span>
          ) : diag.lastPartial ? (
            <span className="text-blue-300">{diag.lastPartial}</span>
          ) : diag.lastFinal ? (
            <span className="text-gray-200">{diag.lastFinal}</span>
          ) : diag.shortcutPressed ? (
            <span className="text-yellow-300">🎤 录音中... ({diag.audioFrames} 帧)</span>
          ) : (
            <span className="text-gray-500">
              {shortcutRegistered ? '✅ 就绪' : '⏳ 注册快捷键'} | 帧: {diag.audioFrames}
            </span>
          )}
        </div>
      </div>

      {/* Recognition results area — horizontal flowing text, wraps to next line */}
      <div className="flex-1 overflow-hidden px-4 pb-3">
        <div className="h-full overflow-y-auto">
          {/* Combine all finalized text into a flowing paragraph, with the latest
              partial appended at the end. This gives the "边说边出字" experience:
              text fills the window width horizontally, then wraps to the next line. */}
          <p
            className="leading-relaxed break-words"
            style={{ fontSize: settings.fontSize, color: 'var(--text-primary, #1f2937)' }}
          >
            {segments.map((segment) => (
              <span key={segment.id}>
                {segment.text}
                {segment.language && (
                  <span className="ml-1 text-[10px] text-gray-400 uppercase align-baseline">
                    {segment.language === 'zh' ? '中' : segment.language}
                  </span>
                )}
              </span>
            ))}
            {currentPartial && (
              <span className="text-gray-400 italic opacity-70">
                {currentPartial}…
              </span>
            )}
          </p>
        </div>
      </div>

      {/* m7 fix: Toast notification for copy feedback */}
      {toastMessage && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0 }}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 max-w-[calc(100%-2rem)] px-3 py-1.5 bg-gray-800/90 text-white text-xs rounded-2xl shadow-lg text-center leading-relaxed break-words"
        >
          {toastMessage}
        </motion.div>
      )}
    </motion.div>
  );
}
