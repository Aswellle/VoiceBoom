// FloatingWindow — the main voice recognition display
// P0: HUD-style interface with 3 states (Idle/Listening/Result)

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { LogicalSize } from '@tauri-apps/api/window';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { useAsr } from '../../hooks/useAsr';
import { HistoryPanel } from '../HistoryPanel';
import { useAppStore, type RecognitionSegment } from '../../stores/useAppStore';
import { copyToClipboard } from '../../utils/clipboard';
import { Waveform } from '../Waveform';
import { useGlobalShortcut } from '../../hooks/useGlobalShortcut';
import { ENGINE_DISPLAY_NAMES } from '../../constants/engines';

// ---------------------------------------------------------------------------
// Window-budget constants (mirror tauri.conf.json so JS and Rust agree).
// ---------------------------------------------------------------------------
const MIN_W = 320;
const MIN_H = 100;
const MAX_W = 900;
const MAX_H = 500;
const DEFAULT_W = 600;
const DEFAULT_H = 140;

// Fallback chrome height (header + paddings). Used until the
// first layout measurement; after that we measure the live chrome via chromeRef.
const CHROME_H = 44 + 12;

// Distance from the top (px) under which we treat the user as "at top".
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
  opacity = 1,
}: {
  segment: RecognitionSegment;
  isNewest: boolean;
  reduceMotion: boolean;
  fontSize: number;
  opacity?: number;
}) {
  const showToast = useAppStore((s) => s.showToast);
  const handleCopy = useCallback(async () => {
    const ok = await copyToClipboard(segment.text);
    showToast(ok ? '已复制到剪贴板' : '复制失败，请手动选取文字');
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
      style={{ fontSize, opacity }}
      title="点击复制到剪贴板"
    >
      <span style={{ color: 'var(--text-primary)' }} className="leading-relaxed">
        {segment.text}
      </span>
      {segment.language && (
        <span className="ml-1.5 text-[10px] uppercase align-baseline" style={{ color: 'var(--text-tertiary)' }}>
          {segment.language === 'zh' ? '中' : segment.language}
        </span>
      )}
      <span className="absolute -top-1 -right-1 opacity-0 group-hover:opacity-100 transition-opacity text-[10px] px-1.5 py-0.5 rounded-full pointer-events-none" style={{ background: 'var(--surface-base)', color: 'var(--text-primary)' }}>
        复制
      </span>
    </motion.div>
  );
}

export function FloatingWindow() {
  const { startListening, stopListening } = useAsr();
  useGlobalShortcut(startListening, stopListening);

  const sessionState = useAppStore((s) => s.sessionState);
  const segments = useAppStore((s) => s.segments);
  const currentPartial = useAppStore((s) => s.currentPartial);
  const settings = useAppStore((s) => s.settings);
  const loadSettings = useAppStore((s) => s.loadSettings);
  const toastMessage = useAppStore((s) => s.toastMessage);
  const settingsLoaded = useAppStore((s) => s.settingsLoaded);
  const [engineReady, setEngineReady] = useState<boolean | null>(null);

  // contentRef wraps the actual rendered segments so we can measure its
  // natural (unconstrained) height. scrollRef is the overflow container.
  // chromeRef wraps the fixed chrome (header) so we can measure
  // its real height instead of relying on a hardcoded constant.
  const contentRef = useRef<HTMLDivElement>(null);
  const chromeRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const lastResizeH = useRef(DEFAULT_H);

  // -----------------------------------------------------------------------
  // P0: Partial rendering optimization — only update DOM content, not window size.
  // Window height changes only on final sentences, not on every partial.
  // -----------------------------------------------------------------------
  const autoResize = useCallback(() => {
    const el = contentRef.current;
    if (!el) return;
    const naturalTextH = el.scrollHeight;
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

  // P0: Only resize on final segments, not on partial updates.
  useLayoutEffect(() => {
    autoResize();
  }, [segments, settings.fontSize, autoResize]);

  // P0: Auto-scroll to top (newest first) so the latest partial is always visible.
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const atTop = el.scrollTop <= SCROLL_THRESHOLD;
    if (atTop || lastResizeH.current < MAX_H) {
      el.scrollTo({ top: 0, behavior: 'instant' });
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
  // Derived state
  // Phase 13: Derive isListening from authoritative sessionState.
  // Do NOT combine status + audioFrames to infer recording state.
  const isListening = sessionState === 'starting' || sessionState === 'recording' || sessionState === 'stopping' || sessionState === 'finalizing';
  const hasContent = segments.length > 0 || currentPartial;

  // P0: HUD state machine — 3 clear states
  const hudState = isListening ? 'listening' : hasContent ? 'result' : 'idle';

  const statusText =
    hudState === 'listening'
      ? '正在聆听…'
      : hudState === 'idle'
      ? engineReady === false
        ? '请先打开「设置」，配置语音引擎后再开始'
        : engineReady === null
        ? '正在检查语音引擎…'
        : '按住快捷键开始说话'
      : '';

  const currentEngineLabel = ENGINE_DISPLAY_NAMES[settings.engine] || settings.engine;

  // P0: Filter segments by HUD density and reverse for newest-first display.
  // Combined into single useMemo to avoid double array allocation.
  const displaySegments = useMemo(() => {
    const count = settings.hudDensity === 'compact' ? 1 : settings.hudDensity === 'standard' ? 5 : segments.length;
    return segments.slice(-count).reverse();
  }, [segments, settings.hudDensity]);

  // #3 fix: derive the effective dark state. In "auto" mode, follow the OS
  // preference so the glass-dark class and background color stay in sync.
  const isDark = useMemo(() => {
    if (settings.theme !== 'auto') return settings.theme === 'dark';
    return window.matchMedia('(prefers-color-scheme: dark)').matches;
  }, [settings.theme]);
  // -----------------------------------------------------------------------
  // Render — P0: HUD with 3 states (Idle/Listening/Result), newest-first
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
        ${hudState === 'listening' ? 'hud-listening' : ''}
        ${isDark ? 'glass-dark' : ''}
      `}
      style={{
        opacity: settings.opacity,
        background: isDark ? 'var(--surface-glass)' : 'var(--surface-glass)',
      }}
    >
      <div ref={chromeRef}>
        {/* ---------------------------------------------------------------- */}
        {/* Header: mic status + waveform + settings (drag handle)            */}
        {/* ---------------------------------------------------------------- */}
        <div
          className="flex items-center gap-3 px-4 pt-3 pb-1 cursor-move shrink-0"
          onMouseDown={() => {
            const win = getCurrentWebviewWindow();
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
                hudState === 'listening' ? 'bg-red-500 recording-dot' : 'bg-gray-300'
              }`}
            />
            {hudState === 'listening' && <Waveform />}
          </div>

          {/* Status text */}
          {statusText && (
            <span
              className="text-xs italic truncate flex-1 min-w-0"
              style={{ color: 'var(--text-secondary)' }}
            >
              {statusText}
            </span>
          )}

          {/* Engine label */}
          <span
            className="text-[10px] px-2 py-0.5 rounded-full whitespace-nowrap shrink-0"
            style={{ background: 'var(--surface-muted)', color: 'var(--text-secondary)' }}
            title={`当前语音识别引擎：${currentEngineLabel}`}
          >
            {currentEngineLabel}
          </span>

          {/* Start/Stop button */}
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => (isListening ? stopListening() : startListening())}
            className={`px-2.5 py-0.5 rounded-full text-[10px] font-medium transition-colors cursor-pointer shrink-0 ${
              hudState === 'listening'
                ? 'bg-red-500 text-white hover:bg-red-600'
                : 'bg-green-500/80 text-white hover:bg-green-600'
            }`}
            title={hudState === 'listening' ? '点击停止录音' : '点击开始录音'}
          >
            {hudState === 'listening' ? '停止' : '说话'}
          </button>

          {/* History */}
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() => useAppStore.getState().setHistoryOpen(true)}
            className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-white/20 transition-colors cursor-pointer shrink-0"
            style={{ color: 'var(--text-secondary)' }}
            title="识别历史"
            aria-label="打开识别历史"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 12a9 9 0 1 0 3-6.7L3 8" />
              <path d="M3 3v5h5" />
              <path d="M12 7v5l3 2" />
            </svg>
          </button>

          {/* Settings */}
          <button
            onMouseDown={(e) => e.stopPropagation()}
            onClick={() =>
              invoke('open_settings').catch((err) =>
                console.error('open_settings failed:', err),
              )
            }
            className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-white/20 transition-colors cursor-pointer shrink-0"
            style={{ color: 'var(--text-secondary)' }}
            title="打开设置"
            aria-label="打开设置"
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <circle cx="12" cy="12" r="3" />
              <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-2 2 2 2 0 0 1-2-2v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1-2-2 2 2 0 0 1 2-2h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.68 1.65 1.65 0 0 0 10 3.17V3a2 2 0 0 1 2-2 2 2 0 0 1 2 2v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06-.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 2 2 2 2 0 0 1-2 2h-.09a1.65 1.65 0 0 0-1.51 1z" />
            </svg>
          </button>
        </div>
      </div>

      {/* ---------------------------------------------------------------- */}
      {/* Recognition results — newest first, HUD-style                     */}
      {/* ---------------------------------------------------------------- */}
      <div className="flex-1 min-h-0 overflow-hidden px-4 pb-3 relative">
        <div ref={scrollRef} className="h-full overflow-y-auto">
          <div ref={contentRef} className="flex flex-col gap-1">
            {/* P0: Partial text — only update DOM, no resize */}
            {currentPartial && (
              <div className="px-3 py-2 shrink-0">
                <span
                  className="italic"
                  style={{ fontSize: settings.fontSize, color: 'var(--text-secondary)' }}
                >
                  {currentPartial}…
                </span>
              </div>
            )}
            {/* P0: Segments in newest-first order with opacity hierarchy */}
            <AnimatePresence initial={false}>
              {displaySegments.map((segment, idx) => (
                <SegmentItem
                  key={segment.id}
                  segment={segment}
                  isNewest={idx === 0}
                  reduceMotion={settings.reduceMotion}
                  fontSize={settings.fontSize}
                  opacity={1 - idx * 0.2}
                />
              ))}
            </AnimatePresence>
          </div>
        </div>
      </div>

      {/* ---------------------------------------------------------------- */}
      {/* History overlay                                                   */}
      {/* ---------------------------------------------------------------- */}
      <HistoryPanel />

      {/* ---------------------------------------------------------------- */}
      {/* Toast feedback                                                    */}
      {/* ---------------------------------------------------------------- */}
      <AnimatePresence>
        {toastMessage && (
          <motion.div
            initial={{ opacity: 0, y: 10 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 10 }}
            className="absolute bottom-4 left-1/2 -translate-x-1/2 max-w-[calc(100%-2rem)] px-3 py-1.5 rounded-2xl shadow-lg text-center leading-relaxed break-words"
            style={{ background: 'var(--surface-base)', color: 'var(--text-primary)', fontSize: '12px' }}
          >
            {toastMessage}
          </motion.div>
        )}
      </AnimatePresence>
    </motion.div>
  );
}
