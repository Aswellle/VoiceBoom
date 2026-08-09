// Animation configuration — Framer Motion variants and transitions
// Centralized animation definitions for consistent motion design

import { Variants, Transition } from 'framer-motion';

/// Standard spring transition for UI elements
export const springTransition: Transition = {
  type: 'spring',
  stiffness: 300,
  damping: 25,
};

/// Gentle fade transition
export const fadeTransition: Transition = {
  duration: 0.2,
  ease: 'easeOut',
};

/// Slide-in from right (for new text segments)
export const slideFromRight: Variants = {
  initial: { opacity: 0, x: 10, scale: 0.95 },
  animate: { opacity: 1, x: 0, scale: 1 },
  exit: { opacity: 0, x: -20, scale: 0.9 },
};

/// Window appear/disappear
export const windowVariants: Variants = {
  initial: { opacity: 0, scale: 0.8 },
  animate: { opacity: 1, scale: 1 },
  exit: { opacity: 0, scale: 0.8 },
};

/// Stagger children animation
export const staggerContainer: Variants = {
  initial: {},
  animate: {
    transition: {
      staggerChildren: 0.05,
    },
  },
};

/// Fade only (for reduced motion preference)
export const fadeOnly: Variants = {
  initial: { opacity: 0 },
  animate: { opacity: 1 },
  exit: { opacity: 0 },
};

/// Get animation variants based on reduced motion preference
export function getVariants(reduceMotion: boolean): Variants {
  return reduceMotion ? fadeOnly : slideFromRight;
}

/// Get transition based on reduced motion preference
export function getTransition(reduceMotion: boolean): Transition {
  return reduceMotion ? { duration: 0 } : springTransition;
}
