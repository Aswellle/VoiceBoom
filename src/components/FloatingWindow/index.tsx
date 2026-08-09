// FloatingWindow — the main voice recognition display
// Shows real-time recognition results with glassmorphism styling
// Features: newest-first layout, text trimming, animations, drag support

import { useCallback } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
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
  const handleClick = useCallback(() => {
    // Copy to clipboard on click
    navigator.clipboard.writeText(segment.text).catch(() => {
      // Fallback: use Tauri clipboard or ignore
    });
  }, [segment.text]);

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
  const setStatus = useAppStore((s) => s.setStatus);

  const isListening = status === 'listening';
  const hasContent = segments.length > 0 || currentPartial;

  // Determine status text
  const statusText = isListening
    ? '正在聆听...'
    : hasContent
    ? ''
    : '按住快捷键说话';

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
      {/* Header: status indicator + waveform */}
      <div className="flex items-center gap-3 px-4 pt-3 pb-1">
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
          <span className="text-xs text-gray-400 italic">{statusText}</span>
        )}
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
    </motion.div>
  );
}
