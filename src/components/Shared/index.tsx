// Shared UI components — reusable elements across the application

import { motion } from 'framer-motion';

/// Icon button component
export function IconButton({
  children,
  onClick,
  active = false,
}: {
  children: React.ReactNode;
  onClick?: () => void;
  active?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      className={`
        w-8 h-8 rounded-full flex items-center justify-center
        transition-colors duration-150
        ${active ? 'bg-blue-100 text-blue-600' : 'hover:bg-gray-100 text-gray-500'}
      `}
    >
      {children}
    </button>
  );
}

/// Tooltip wrapper
export function Tooltip({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <div className="relative group">
      {children}
      <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 px-2 py-1 bg-gray-800 text-white text-xs rounded opacity-0 group-hover:opacity-100 transition-opacity pointer-events-none whitespace-nowrap">
        {label}
      </div>
    </div>
  );
}

/// Animated container for list items
export function AnimatedItem({
  children,
  delay = 0,
}: {
  children: React.ReactNode;
  delay?: number;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 5 }}
      animate={{ opacity: 1, y: 0 }}
      transition={{ duration: 0.15, delay }}
    >
      {children}
    </motion.div>
  );
}
