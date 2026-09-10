// Waveform component — optimized audio level visualization
// P1: Uses canvas for low-cost real-time drawing instead of 12 Framer Motion nodes

import { useEffect, useRef } from 'react';
import { useAppStore } from '../../stores/useAppStore';

const BAR_COUNT = 12;
const THROTTLE_MS = 30; // ~30 FPS throttle for audio level updates

export function Waveform() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const audioLevel = useAppStore((s) => s.audioLevel);
  const sessionState = useAppStore((s) => s.sessionState);
  const reduceMotion = useAppStore((s) => s.settings.reduceMotion);
  const isListening = sessionState === 'starting' || sessionState === 'recording' || sessionState === 'stopping' || sessionState === 'finalizing';

  // P1: Throttled canvas drawing — decouples rendering from React state updates
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext('2d');
    if (!ctx) return;

    const dpr = window.devicePixelRatio || 1;
    const rect = canvas.getBoundingClientRect();
    canvas.width = rect.width * dpr;
    canvas.height = rect.height * dpr;
    ctx.scale(dpr, dpr);

    const width = rect.width;
    const height = rect.height;
    const barWidth = 3;
    const gap = 3;
    const totalBarWidth = barWidth + gap;

    ctx.clearRect(0, 0, width, height);

    for (let i = 0; i < BAR_COUNT; i++) {
      // Generate a wave-like pattern based on position and audio level
      const baseHeight = Math.sin((i / BAR_COUNT) * Math.PI);
      const barHeight = isListening
        ? Math.max(4, baseHeight * height * (0.4 + audioLevel * 0.6))
        : 4;

      const x = i * totalBarWidth + (width - BAR_COUNT * totalBarWidth) / 2;
      const y = (height - barHeight) / 2;

      ctx.fillStyle = isListening
        ? 'rgba(239, 68, 68, 0.8)'
        : 'rgba(128, 128, 128, 0.4)';
      ctx.beginPath();
      ctx.roundRect(x, y, barWidth, barHeight, 1.5);
      ctx.fill();
    }
  }, [audioLevel, isListening, reduceMotion]);

  return (
    <canvas
      ref={canvasRef}
      className="h-8 w-20"
      style={{ imageRendering: 'pixelated' }}
    />
  );
}
