// FloatingWindow — the main voice recognition display
// Shows real-time recognition results with glassmorphism styling
// Features: newest-first layout, text trimming, animations, drag support

import { useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { invoke } from '@tauri-apps/api/core';
import { useAppStore, RecognitionSegment } from '../../stores/useAppStore';
import { Waveform } from '../Waveform';
import { useGlobalShortcut } from '../../hooks/useGlobalShortcut';

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
  useGlobalShortcut();

  const status = useAppStore((s) => s.status);
  const segments = useAppStore((s) => s.segments);
  const currentPartial = useAppStore((s) => s.currentPartial);
  const settings = useAppStore((s) => s.settings);
  const toastMessage = useAppStore((s) => s.toastMessage);
  const setStatus = useAppStore((s) => s.setStatus);

  const isListening = status === 'listening';
  const hasContent = segments.length > 0 || currentPartial;

  // Determine status text
  const statusText = isListening
    ? '正在聆听...'
    : hasContent
    ? ''
    : '按住快捷键说话';

  // Engine display label
  const engineLabel: Record<string, string> = {
    openai_whisper: 'Whisper API',
    deepgram: 'Deepgram',
    whisper_cpp: 'Whisper 本地',
    funasr: 'FunASR 本地',
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

        {/* Current engine indicator */}
        <span className="text-[10px] px-2 py-0.5 bg-white/30 rounded-full text-gray-500 whitespace-nowrap" title={`当前引擎: ${currentEngineLabel}`}>
          {currentEngineLabel}
        </span>

        {/* Settings button */}
        <button
          onMouseDown={(e) => {
            // Stop propagation so the parent drag handle doesn't intercept the click
            e.stopPropagation();
            e.preventDefault();
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

      {/* Recognition results area */}
      <div className="flex-1 overflow-hidden px-4 pb-3">
        <AnimatePresence mode="popLayout">
          {/* Partial (in-progress) text */}
          {currentPartial && (
            <motion.div
              key="partial"
              initial={settings.reduceMotion ? false : { opacity: 0 }}
              animate={{ opacity: 0.6 }}
              exit={{ opacity: 0 }}
              className="mb-2"
            >
              <span
                className="text-gray-500 dark:text-gray-400 italic"
                style={{ fontSize: settings.fontSize * 0.9 }}
              >
                {currentPartial}...
              </span>
            </motion.div>
          )}

          {/* Finalized segments — newest first */}
          {[...segments].reverse().map((segment, index) => (
            <SegmentItem
              key={segment.id}
              segment={segment}
              isNewest={index === 0}
              reduceMotion={settings.reduceMotion}
              fontSize={settings.fontSize}
            />
          ))}
        </AnimatePresence>
      </div>

      {/* m7 fix: Toast notification for copy feedback */}
      {toastMessage && (
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0 }}
          className="absolute bottom-4 left-1/2 -translate-x-1/2 px-3 py-1.5 bg-gray-800/90 text-white text-xs rounded-full shadow-lg"
        >
          {toastMessage}
        </motion.div>
      )}
    </motion.div>
  );
}
