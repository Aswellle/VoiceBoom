// Shared UI primitives for Settings panel.
// Extracted from Settings/index.tsx to reduce file size and improve maintainability.

import type { ReactNode } from 'react';

/// Slider component — accessible range input with label and value display.
export function Slider({
  label,
  value,
  min,
  max,
  step = 1,
  onChange,
  unit = '',
}: {
  label: string;
  value: number;
  min: number;
  max: number;
  step?: number;
  onChange: (v: number) => void;
  unit?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <div className="flex justify-between gap-3 text-sm">
        <span className="text-gray-600 min-w-0">{label}</span>
        <span className="text-gray-400 shrink-0 whitespace-nowrap">
          {value}
          {unit}
        </span>
      </div>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="w-full accent-blue-500"
      />
    </div>
  );
}

/// Select component — accessible dropdown with proper labeling.
export function Select({
  label,
  value,
  options,
  onChange,
}: {
  label: string;
  value: string;
  options: { value: string; label: string }[];
  onChange: (v: string) => void;
}) {
  return (
    <div className="flex flex-col gap-1">
      {label && <label className="text-sm" style={{ color: 'var(--text-primary)' }}>{label}</label>}
      <select
        value={value}
        onChange={(e) => onChange(e.target.value)}
        className="rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2"
        style={{
          borderColor: 'var(--border-subtle)',
          background: 'var(--surface-base)',
          color: 'var(--text-primary)',
        }}
      >
        {options.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
    </div>
  );
}

/// Text input component — accessible input with proper labeling.
export function TextInput({
  label,
  value,
  onChange,
  placeholder,
  type = 'text',
  disabled = false,
  helpText,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  placeholder?: string;
  type?: string;
  disabled?: boolean;
  helpText?: string;
}) {
  return (
    <div className="flex flex-col gap-1">
      <label className="text-sm" style={{ color: 'var(--text-primary)' }}>{label}</label>
      <input
        type={type}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        disabled={disabled}
        className={`rounded-lg border px-3 py-2 text-sm focus:outline-none focus:ring-2 ${
          disabled ? 'opacity-50 cursor-not-allowed' : ''
        }`}
        style={{
          borderColor: 'var(--border-subtle)',
          background: disabled ? 'var(--surface-muted)' : 'var(--surface-base)',
          color: 'var(--text-primary)',
        }}
      />
      {helpText && <p className="text-xs mt-0.5" style={{ color: 'var(--text-tertiary)' }}>{helpText}</p>}
    </div>
  );
}

/// Toggle component — accessible switch with proper ARIA semantics.
export function Toggle({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (v: boolean) => void;
}) {
  return (
    <label className="flex items-center justify-between gap-3 cursor-pointer py-2">
      <span className="text-sm min-w-0" style={{ color: 'var(--text-primary)' }}>{label}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        onClick={() => onChange(!checked)}
        className={`
          relative w-11 h-6 shrink-0 rounded-full transition-colors
          focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-offset-2
          ${checked ? 'bg-blue-500' : 'bg-gray-300'}
        `}
        style={{
          backgroundColor: checked ? 'var(--accent)' : 'var(--border-subtle)',
        }}
      >
        <span
          className={`
            absolute top-1 w-4 h-4 rounded-full bg-white shadow transition-transform
            ${checked ? 'translate-x-6' : 'translate-x-1'}
          `}
        />
      </button>
    </label>
  );
}

/// Section wrapper — groups related settings with a title.
export function Section({
  title,
  children,
  description,
}: {
  title: string;
  children: ReactNode;
  description?: string;
}) {
  return (
    <div className="flex flex-col gap-3">
      <div>
        <h3 className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{title}</h3>
        {description && (
          <p className="text-xs mt-0.5" style={{ color: 'var(--text-tertiary)' }}>{description}</p>
        )}
      </div>
      {children}
    </div>
  );
}
