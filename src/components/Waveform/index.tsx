// Waveform component — animated audio level visualization
// Displays a series of bars that animate based on audio input level

import { motion } from 'framer-motion';
import { useAppStore } from '../../stores/useAppStore';

const BAR_COUNT = 12;

export function Waveform() {
  // Single selector instead of three — avoids up to 3 separate re-renders per
  // store change.
  const { audioLevel, status, reduceMotion } = useAppStore((s) => ({
    audioLevel: s.audioLevel,
    status: s.status,
    reduceMotion: s.settings.reduceMotion,
  }));

  const isListening = status === 'listening';

  return (
    <div className="flex items-center justify-center gap-[3px] h-8">
      {Array.from({ length: BAR_COUNT }).map((_, i) => {
        // Generate a wave-like pattern based on position and audio level
        const baseHeight = Math.sin((i / BAR_COUNT) * Math.PI) * 100;
        const animatedHeight = isListening
          ? Math.max(15, baseHeight * (0.4 + audioLevel * 0.6))
          : 15;

        return (
          <motion.div
            key={i}
            className="w-[3px] rounded-full"
            style={{
              backgroundColor: isListening
                ? 'rgba(239, 68, 68, 0.8)'
                : 'rgba(128, 128, 128, 0.4)',
            }}
            animate={{
              height: animatedHeight,
            }}
            transition={{
              duration: reduceMotion ? 0 : 0.15,
              ease: 'easeOut',
              delay: reduceMotion ? 0 : i * 0.02,
            }}
          />
        );
      })}
    </div>
  );
}
