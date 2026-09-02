// FloatingWindow — the main voice recognition display
// Shows real-time recognition results with glassmorphism styling.
// Auto-resizes downward as transcribed text grows, up to a maximum viewport,
// then scrolls to keep the latest output visible.

import { useCallback, useEffect, useLayoutEffect, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { LogicalSize } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAppStore, RecognitionSegment } from '../../stores/useAppStore';
import { Waveform } from '../Waveform';
import { useGlobalShortcut } from '../../hooks/useGlobalShortcut';
import { useAsr } from '../../hooks/useAsr';
import { HistoryPanel } from '../HistoryPanel';

// ---------------------------------------------------------------------------
// Window-budget constants (mirror tauri.conf.json so JS and Rust agree).
// ---------------------------------------------------------------------------
const MIN_W = 320;
const MIN_H = 100;
const MAX_W = 900;
const MAX_H = 500;
const DEFAULT_W = 600;
const DEFAULT_H = 140;

// Fallback chrome height (header + diagnostic + paddings). Used until the
// first layout measurement; after that we measure the live chrome via chromeRef.
const CHROME_H = 44 + 18 + 12;

// Distance from the bottom (px) under which we treat the user as "at bottom".
const SCROLL_THRESHOLD = 40;

// ---------------------------------------------------------------------------
// Animation variants
// ---------------------------------------------------------------------------
const containerVariants = {
  initial: { opacity: 0, scale: 0.85 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.85 },
};

const segmentVariants = {
  initial: { opacity: 0, x: 10, scale: 0.95 },
  animate: { opacity: 1, x: 0, scale: 1 },
  exit: { opacity: 0, x: -20, scale: 0.9 },
};

// ---------------------------------------------------------------------------
// Single recognition segment display
// ---------------------------------------------------------------------------
export function SegmentItem({
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
  const handleCopy = useCallback(() => {
    const text = segment.text;
    const ok = () => showToast('已复制到剪贴板');
    const fail = () => showToast('复制失败，请手动选取文字');
    // Tauri 生产环境走 asset:// 协议，navigator.clipboard 可能不可用，
    // 此时回退到 textarea + execCommand，保证点击复制始终可用。
    if (navigator.clipboard?.writeText) {
      navigator.clipboard.writeText(text).then(ok).catch(fail);
    } else {
      try {
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.style.position = 'fixed';
        ta.style.opacity = '0';
        document.body.appendChild(ta);
        ta.select();
        document.execCommand('copy');
        document.body.removeChild(ta);
        ok();
      } catch {
        fail();
      }
    }
  }, [segment.text, showToast]);

  // 仅用一句话描述操作结果，避免把长文本读出来。
  const ariaLabel = `复制识别结果：${segment.text.slice(0, 20)}${segment.text.length > 20 ? '…' : ''}`;

  return (
    <motion.div
      role="button"
      tabIndex={0}
      aria-label={ariaLabel}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault();
          handleCopy();
        }
      }}
      variants={segmentVariants}
      initial={reduceMotion ? false : 'initial'}
      animate="animate"
      exit="exit"
      transition={{ type: 'spring', stiffness: 400, damping: 25 }}
      layout={reduceMotion ? false : true}
      onClick={handleCopy}
      className={`group relative px-3 py-2 rounded-2xl cursor-pointer transition-colors shrink-0 ${
        isNewest ? 'bg-blue-500/8 hover:bg-blue-500/15' : 'hover:bg-white/30'
      }`}
      style={{ fontSize }}
      title="点击复制到剪贴板"
    >
      <span className="text-gray-800 dark:text-gray-100 leading-relaxed">
        {segment.text}
      </span>
      {segment.language && (
        <span className="ml-1.5 text-[10px] text-gray-400 uppercase align-baseline">
          {segment.language === 'zh' ? '中' : segment.language}
        </span>
      )}
      <span className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 transition-opacity text-[10px] bg-gray-700/80 text-white px-1.5 py-0.5 rounded-full pointer-events-none">
        复制
      </span>
    </motion.div>
  );
}

export function FloatingWindow() {
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

  const [engineReady, setEngineReady] = useState<boolean | null>(null);
  const [diag, setDiag] = useState({ shortcutPressed: false, audioFrames: 0 });
  // canScrollUp drives the "scroll to bottom" FAB — shows when the user has
  // scrolled up to read older text, letting them quickly return to live output.
  const [canScrollUp, setCanScrollUp] = useState(false);

  // -----------------------------------------------------------------------
  // Scroll listener: track whether the user has scrolled away from the bottom
  // -----------------------------------------------------------------------
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const onScroll = () => {
      setCanScrollUp(el.scrollHeight - el.scrollTop - el.clientHeight >= SCROLL_THRESHOLD);
    };
    el.addEventListener('scroll', onScroll, { passive: true });
    return () => el.removeEventListener('scroll', onScroll);
  }, []);

  // -----------------------------------------------------------------------

  // contentRef wraps the actual rendered segments so we can measure its
  // natural (unconstrained) height. scrollRef is the overflow container.
  // chromeRef wraps the fixed chrome (header + diagnostic) so we can measure
  // its real height instead of relying on a hardcoded constant.
  const contentRef = useRef<HTMLDivElement>(null);
  const chromeRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastResizeH = useRef(DEFAULT_H);

  // -----------------------------------------------------------------------
  // Auto-resize: measure the content's natural height and grow the window
  // to fit, clamped to [MIN_H, MAX_H]. Anchored at the top so it grows
  // downward.
  // -----------------------------------------------------------------------
  const autoResize = useCallback(() => {
    const el = contentRef.current;
    if (!el) return;
    const naturalTextH = el.scrollHeight;
    // Measure the live chrome height (header + diagnostic) so the text area
    // budget stays correct even if font/DPI/wrapping shifts the chrome.
    const chromeH = chromeRef.current
      ? chromeRef.current.getBoundingClientRect().height
      : CHROME_H;
    const desired = Math.round(chromeH + naturalTextH);
    const clamped = Math.min(MAX_H, Math.max(MIN_H, desired));

    // Only call setSize when the value actually changed — avoids a Tauri
    // round-trip (and potential flicker) on every keystroke.
    if (clamped === lastResizeH.current) return;
    lastResizeH.current = clamped;

    getCurrentWebviewWindow()
      .setSize(new LogicalSize(DEFAULT_W, clamped))
      .catch(() => {});
  }, []);

  // Measure after every render that could change content height.
  useLayoutEffect(() => {
    autoResize();
  }, [segments, currentPartial, settings.fontSize, autoResize]);

  // -----------------------------------------------------------------------
  // Auto-scroll to the bottom so the latest partial is always visible once
  // the window has hit its max height.
  // -----------------------------------------------------------------------
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    // If the user is near the bottom (or the window is still growing), keep
    // them pinned to the latest text. A small tolerance avoids fighting the
    // user if they scroll up to read older content.
    //
    // Use instant scrolling: this fires on every ASR partial update, and
    // queued smooth-scrolls would otherwise overlap and stutter (jitter) the
    // view during fast dictation.
    const atBottom =
      el.scrollHeight - el.scrollTop - el.clientHeight < SCROLL_THRESHOLD;
    if (atBottom || lastResizeH.current < MAX_H) {
      el.scrollTo({ top: el.scrollHeight, behavior: 'instant' });
    }
  }, [segments, currentPartial]);

  // -----------------------------------------------------------------------
  // Engine readiness check
  // -----------------------------------------------------------------------
  useEffect(() => {
    if (!settingsLoaded) return;
    invoke<{
      is_local: boolean;
      model_installed?: boolean;
      tokens_installed?: boolean;
      vad_installed?: boolean;
    }>('switch_engine', { engine: settings.engine })
      .then((result) => {
         const isLocal = result.is_local;
         const ready = isLocal
           ? Boolean(result.model_installed && result.tokens_installed && result.vad_installed)
           : Boolean(settings.apiKey);
         setEngineReady(ready);
       })
      .catch(() => setEngineReady(false));
  }, [settings.engine, settings.apiKey, settingsLoaded]);
  // -----------------------------------------------------------------------
  // Restore the persisted window position once settings are loaded.
  // -----------------------------------------------------------------------
  useEffect(() => {
    if (!settingsLoaded) return;
    // loadSettings already populated windowPosition; now apply it to the window.
    useAppStore.getState().restoreWindowPosition();
  }, [settingsLoaded]);


  // -----------------------------------------------------------------------
  // Reload settings when the engine is switched so the label stays current
  // -----------------------------------------------------------------------
  useEffect(() => {
    const unlisten = listen('engine:switched', () => {
      loadSettings().catch(console.error);
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [loadSettings]);

  // -----------------------------------------------------------------------
  // Shortcut + ASR heartbeat listeners (diagnostic only)
  // -----------------------------------------------------------------------
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

  useEffect(() => {
    const unHeartbeat = listen<{ frames: number }>('asr:heartbeat', (e) => {
      setDiag((d) => ({ ...d, audioFrames: e.payload.frames }));
    });
    return () => {
      unHeartbeat.then((f) => f());
    };
  }, []);

  // -----------------------------------------------------------------------
  // Derived state
  // -----------------------------------------------------------------------
  const isListening = status === 'listening';
  const hasContent = segments.length > 0 || currentPartial;

  const statusText = isListening
    ? '正在聆听…'
    : hasContent
    ? ''
    : engineReady === false
    ? '请先打开「设置」，配置语音引擎后再开始'
    : engineReady === null
    ? '正在检查语音引擎…'
    : '按住快捷键开始说话';

  const engineLabel: Record<string, string> = {
    openai_whisper: 'Whisper API',
    deepgram: 'Deepgram',
    whisper_cpp: 'SenseVoice',
    funasr: 'SenseVoice',
  };
  const currentEngineLabel = engineLabel[settings.engine] || settings.engine;

  // -----------------------------------------------------------------------
  // Render
  // -----------------------------------------------------------------------
  return (
    <motion.div
      variants={containerVariants}
      initial={settings.reduceMotion ? false : 'initial'}
      animate="animate"
      exit="exit"
      transition={{ type: 'spring', stiffness: 300, damping: 25 }}
      className={`
        glass w-full h-full flex flex-col select-none
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
      <div ref={chromeRef}>
      {/* ---------------------------------------------------------------- */}
      {/* Header: status + waveform + engine + controls (drag handle)        */}
      {/* ---------------------------------------------------------------- */}
      <div
        className="flex items-center gap-3 px-4 pt-3 pb-1 cursor-move shrink-0"
        onMouseDown={() => {
          const win = getCurrentWebviewWindow();
          // startDragging() resolves when the drag ends — then persist the new
          // position so the floating window reopens where the user left it.
          win.startDragging().then(() => {
            win.position().then((pos) => {
              useAppStore.getState().setWindowPosition({ x: pos.x, y: pos.y });
              useAppStore.getState().persistWindowPosition();
            }).catch(() => {});
          }).catch(() => {});
        }}
      >
        {/* Recording indicator + waveform */}
        <div className="flex items-center gap-2">
          <div
            className={`w-2.5 h-2.5 rounded-full transition-colors ${
              isListening ? 'bg-red-500 recording-dot' : 'bg-gray-300'
            }`}
          />
          {isListening && <Waveform />}
        </div>

        {/* Status text */}
        {statusText && (
          <span className="text-xs text-gray-400 italic truncate flex-1 min-w-0">
            {statusText}
          </span>
        )}

        {/* Diagnostic dots */}
        <div
          className="flex items-center gap-1.5 text-[10px] shrink-0"
          title="状态指示：快捷键 / 音频 / 识别"
        >
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              shortcutRegistered ? 'bg-green-400' : 'bg-red-400'
            }`}
            title={shortcutRegistered ? '快捷键已就绪' : '快捷键未就绪'}
          />
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              diag.shortcutPressed ? 'bg-red-500 animate-pulse' : 'bg-gray-400'
            }`}
            title={diag.shortcutPressed ? '正在收音…' : '已松开快捷键'}
          />
          <span
            className={`w-1.5 h-1.5 rounded-full ${
              diag.audioFrames > 0 ? 'bg-blue-400' : 'bg-gray-400'
            }`}
            title={
              diag.audioFrames > 0
                ? `已收到 ${diag.audioFrames} 帧音频`
                : '等待音频输入…'
            }
          />
          {toastMessage && (
            <span className="w-1.5 h-1.5 rounded-full bg-red-600" title={toastMessage} />
          )}
        </div>

        {/* Engine label */}
        <span
          className="text-[10px] px-2 py-0.5 bg-white/30 rounded-full text-gray-500 whitespace-nowrap shrink-0"
          title={`当前语音识别引擎：${currentEngineLabel}`}
        >
          {currentEngineLabel}
        </span>
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => (isListening ? stopListening() : startListening())}
          className={`px-2.5 py-0.5 rounded-full text-[10px] font-medium transition-colors cursor-pointer shrink-0 ${
            isListening
              ? 'bg-red-500 text-white hover:bg-red-600'
              : 'bg-green-500/80 text-white hover:bg-green-600'
          }`}
          title={isListening ? '点击停止录音' : '点击开始录音'}
        >
          {isListening ? '停止' : '说话'}
        </button>

        {/* History */}
        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() => useAppStore.getState().setHistoryOpen(true)}
          className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-white/20 transition-colors text-gray-400 hover:text-gray-600 cursor-pointer shrink-0"
          title="识别历史"
          aria-label="打开识别历史"
        >
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
            <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
            <path d="M3 3v5h5" />
            <path d="M12 7v5l3 2" />
          </svg>
        </button>

        <button
          onMouseDown={(e) => e.stopPropagation()}
          onClick={() =>
            invoke('open_settings').catch((err) =>
              console.error('open_settings failed:', err),
            )
          }
          className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-white/20 transition-colors text-gray-400 hover:text-gray-600 cursor-pointer shrink-0"
          title="打开设置"
          aria-label="打开设置"
        >
          <svg
            width="14"
            height="14"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            strokeLinecap="round"
            strokeLinejoin="round"
          >
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68 1.65 1.65 0 0 0 10 3.17V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </button>
      </div>

      {/* ---------------------------------------------------------------- */}
      {/* Diagnostic text                                                    */}
      {/* ---------------------------------------------------------------- */}
      <div className="px-4 pb-1 shrink-0">
        <div className="text-[10px] text-gray-400 font-mono leading-tight truncate">
          {toastMessage ? (
            <span className="text-red-400">❌ {toastMessage}</span>
          ) : currentPartial ? (
            <span className="text-blue-300">{currentPartial}</span>
          ) : diag.shortcutPressed ? (
            <span className="text-yellow-300">
              🎤 正在录音…
            </span>
          ) : (
            <span className="text-gray-500">
              {shortcutRegistered
                ? '✅ 已就绪，按住快捷键即可说话'
                : '⏳ 正在注册快捷键…'}
            </span>
          )}
        </div>
      </div>
      </div>

      {/* ---------------------------------------------------------------- */}
      {/* Recognition results — overflow scroll, auto-resizes window to fit */}
      {/* ---------------------------------------------------------------- */}
      <div className="flex-1 min-h-0 overflow-hidden px-4 pb-3 relative">
        {/* Top fade — hints that older content is above */}
        <div
          className={`pointer-events-none absolute top-0 left-4 right-4 h-6 z-10 bg-gradient-to-b from-black/10 to-transparent transition-opacity duration-300 ${
            canScrollUp ? 'opacity-100' : 'opacity-0'
          }`}
        />
        <div ref={scrollRef} className="h-full overflow-y-auto">
          <div ref={contentRef} className="flex flex-col gap-1">
            <AnimatePresence initial={false}>
              {segments.map((segment, idx) => (
                <SegmentItem
                  key={segment.id}
                  segment={segment}
                  isNewest={idx === segments.length - 1}
                  reduceMotion={settings.reduceMotion}
                  fontSize={settings.fontSize}
                />
              ))}
            </AnimatePresence>
            {currentPartial && (
              <motion.div
                key="partial"
                initial={{ opacity: 0.5 }}
                animate={{ opacity: 0.7 }}
                className="px-3 py-2 rounded-2xl bg-gray-400/5 shrink-0"
              >
                <span
                  className="text-gray-400 italic"
                  style={{ fontSize: settings.fontSize }}
                >
                  {currentPartial}…
                </span>
              </motion.div>
            )}
          </div>
        </div>
        {/* Scroll-to-bottom FAB — appears when user has scrolled up */}
        <AnimatePresence>
          {canScrollUp && (
            <motion.button
              animate={{ opacity: 1, scale: 1 }}
              exit={{ opacity: 0, scale: 0.8 }}
              onClick={() => {
                scrollRef.current?.scrollTo({
                  top: scrollRef.current.scrollHeight,
                  behavior: 'smooth',
                });
              }}
              className="absolute bottom-3 left-1/2 -translate-x-1/2 w-8 h-8 flex items-center justify-center bg-gray-800/80 hover:bg-gray-700/90 text-white rounded-full shadow-lg cursor-pointer z-20"
              title="返回最新内容"
              aria-label="返回最新内容"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                <polyline points="6 9 12 15 18 9" />
              </svg>
            </motion.button>
          )}
        </AnimatePresence>
      </div>

      {/* ---------------------------------------------------------------- */}
      {/* History overlay                                                   */}
      {/* ---------------------------------------------------------------- */}
      <HistoryPanel />

      {/* ---------------------------------------------------------------- */}
      <AnimatePresence>
        {toastMessage && (
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 10 }}
            className="absolute bottom-4 left-1/2 -translate-x-1/2 max-w-[calc(100%-2rem)] px-3 py-1.5 bg-gray-800/90 text-white text-xs rounded-2xl shadow-lg text-center leading-relaxed break-words"
          >
            {toastMessage}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}
