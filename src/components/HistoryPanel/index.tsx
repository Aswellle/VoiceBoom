// HistoryPanel — floating-window overlay that shows recognition history.
// Supports search, per-row copy, select-all and clear-all. Opened from the
// floating window header's history button.

import { useEffect, useMemo, useRef, useState } from 'react';
import { motion, AnimatePresence } from 'framer-motion';
import { useAppStore, HistoryRecord } from '../../stores/useAppStore';

/// Format a millisecond epoch into a local HH:MM:SS label.
function formatTime(ms: number): string {
  const d = new Date(ms);
  const p = (n: number) => n.toString().padStart(2, '0');
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

const LANG_LABELS: Record<string, string> = {
  zh: '中',
  en: 'EN',
  ja: '日',
  ko: '한',
};

function languageLabel(code: string | null): string {
  if (!code) return '';
  return LANG_LABELS[code] ?? code.toUpperCase();
}

export function HistoryPanel() {
  const history = useAppStore((s) => s.history);
  const isHistoryOpen = useAppStore((s) => s.isHistoryOpen);
  const setHistoryOpen = useAppStore((s) => s.setHistoryOpen);
  const loadHistory = useAppStore((s) => s.loadHistory);
  const clearHistory = useAppStore((s) => s.clearHistory);
  const showToast = useAppStore((s) => s.showToast);
  const reduceMotion = useAppStore((s) => s.settings.reduceMotion);

  const [query, setQuery] = useState('');
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [confirmClear, setConfirmClear] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  // Load history the first time the panel opens.
  useEffect(() => {
    if (isHistoryOpen) {
      loadHistory();
      // Focus the search box so the user can type immediately.
      setTimeout(() => searchRef.current?.focus(), 50);
    } else {
      // Reset transient state on close.
      setQuery('');
      setSelected(new Set());
      setConfirmClear(false);
    }
  }, [isHistoryOpen, loadHistory]);

  // Filter by search query (case-insensitive substring on text).
  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return history;
    return history.filter((r) => r.text.toLowerCase().includes(q));
  }, [history, query]);

  const toggleSelect = (id: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  const toggleSelectAll = () => {
    if (selected.size === filtered.length) {
      setSelected(new Set());
    } else {
      setSelected(new Set(filtered.map((r) => r.id)));
    }
  };

  const copyRecord = async (rec: HistoryRecord) => {
    const ok = () => showToast('已复制到剪贴板');
    const fail = () => showToast('复制失败，请手动选取文字');
    if (navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(rec.text);
        ok();
      } catch {
        fail();
      }
    } else {
      try {
        const ta = document.createElement('textarea');
        ta.value = rec.text;
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
  };

  const copySelected = async () => {
    const texts = filtered.filter((r) => selected.has(r.id)).map((r) => r.text);
    if (texts.length === 0) return;
    const joined = texts.join('\n');
    const ok = () => showToast(`已复制 ${texts.length} 条记录`);
    const fail = () => showToast('复制失败');
    if (navigator.clipboard?.writeText) {
      try {
        await navigator.clipboard.writeText(joined);
        ok();
      } catch {
        fail();
      }
    } else {
      fail();
    }
  };

  const handleClear = async () => {
    if (!confirmClear) {
      setConfirmClear(true);
      return;
    }
    await clearHistory();
    setSelected(new Set());
  };

  return (
    <AnimatePresence>
      {isHistoryOpen && (
        <motion.div
          initial={reduceMotion ? false : { opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.15 }}
          className="absolute inset-0 z-40 flex flex-col glass overflow-hidden"
          style={{
            background: 'rgba(255, 255, 255, 0.92)',
            backdropFilter: 'blur(30px)',
          }}
        >
          {/* Header */}
          <div className="flex items-center gap-2 px-4 pt-3 pb-2 shrink-0">
            <span className="text-sm font-medium text-gray-700">识别历史</span>
            <span className="text-xs text-gray-400">
              {query ? `${filtered.length} / ${history.length}` : `${history.length} 条`}
            </span>
            <div className="flex-1" />
            <button
              onClick={() => setHistoryOpen(false)}
              className="w-6 h-6 flex items-center justify-center rounded-full hover:bg-black/10 text-gray-500 cursor-pointer"
              title="关闭"
              aria-label="关闭历史记录"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                <line x1="18" y1="6" x2="6" y2="18" />
                <line x1="6" y1="6" x2="18" y2="18" />
              </svg>
            </button>
          </div>

          {/* Search + actions */}
          <div className="flex items-center gap-2 px-4 pb-2 shrink-0">
            <input
              ref={searchRef}
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="搜索识别文本…"
              className="flex-1 min-w-0 px-3 py-1.5 text-xs rounded-lg bg-white/70 border border-gray-200 focus:outline-none focus:border-blue-400 text-gray-700 placeholder-gray-400"
            />
            {selected.size > 0 && (
              <button
                onClick={copySelected}
                className="px-2.5 py-1 text-[10px] rounded-full bg-blue-500 text-white hover:bg-blue-600 cursor-pointer whitespace-nowrap"
                title="复制所选记录"
              >
                复制所选 ({selected.size})
              </button>
            )}
            <button
              onClick={toggleSelectAll}
              className="px-2.5 py-1 text-[10px] rounded-full bg-gray-200 text-gray-600 hover:bg-gray-300 cursor-pointer whitespace-nowrap"
            >
              {selected.size === filtered.length && filtered.length > 0 ? '取消全选' : '全选'}
            </button>
            <button
              onClick={handleClear}
              className={`px-2.5 py-1 text-[10px] rounded-full cursor-pointer whitespace-nowrap transition-colors ${
                confirmClear
                  ? 'bg-red-500 text-white hover:bg-red-600'
                  : 'bg-gray-200 text-gray-600 hover:bg-gray-300'
              }`}
              title={confirmClear ? '再次点击确认清空' : '清空所有历史记录'}
            >
              {confirmClear ? '确认清空?' : '清空'}
            </button>
          </div>

          {/* List */}
          <div className="flex-1 min-h-0 overflow-y-auto px-4 pb-3">
            {filtered.length === 0 ? (
              <div className="flex flex-col items-center justify-center h-full text-gray-400 text-xs gap-1">
                {history.length === 0 ? (
                  <>
                    <span className="text-2xl">📝</span>
                    <span>还没有识别记录</span>
                    <span className="text-[10px]">按住快捷键说话，识别结果会出现在这里</span>
                  </>
                ) : (
                  <>
                    <span className="text-2xl">🔍</span>
                    <span>没有匹配 "{query}" 的记录</span>
                  </>
                )}
              </div>
            ) : (
              <div className="flex flex-col gap-1.5">
                {filtered.map((rec) => (
                  <HistoryRow
                    key={rec.id}
                    record={rec}
                    selected={selected.has(rec.id)}
                    onToggle={() => toggleSelect(rec.id)}
                    onCopy={() => copyRecord(rec)}
                  />
                ))}
              </div>
            )}
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function HistoryRow({
  record,
  selected,
  onToggle,
  onCopy,
}: {
  record: HistoryRecord;
  selected: boolean;
  onToggle: () => void;
  onCopy: () => void;
}) {
  return (
    <div
      className={`group flex items-start gap-2 p-2 rounded-lg cursor-pointer transition-colors ${
        selected ? 'bg-blue-500/15' : 'bg-white/60 hover:bg-white/90'
      }`}
      onClick={onToggle}
      title="点击选择，双击复制"
      onDoubleClick={(e) => {
        e.stopPropagation();
        onCopy();
      }}
    >
      {/* Selection indicator */}
      <div
        className={`mt-1 w-3 h-3 shrink-0 rounded-sm border flex items-center justify-center ${
          selected ? 'bg-blue-500 border-blue-500' : 'border-gray-300'
        }`}
      >
        {selected && (
          <svg width="8" height="8" viewBox="0 0 24 24" fill="none" stroke="white" strokeWidth="3">
            <polyline points="20 6 9 17 4 12" />
          </svg>
        )}
      </div>

      {/* Content */}
      <div className="flex-1 min-w-0">
        <p className="text-xs text-gray-800 leading-relaxed break-words">{record.text}</p>
        <div className="flex items-center gap-1.5 mt-1 text-[10px] text-gray-400">
          <span>{formatTime(record.created_at)}</span>
          {record.language && <span>· {languageLabel(record.language)}</span>}
          {record.engine && <span>· {record.engine}</span>}
          {record.confidence != null && (
            <span>· {(record.confidence * 100).toFixed(0)}%</span>
          )}
        </div>
      </div>

      {/* Copy button (visible on hover) */}
      <button
        onClick={(e) => {
          e.stopPropagation();
          onCopy();
        }}
        className="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity px-2 py-0.5 text-[10px] rounded-full bg-gray-200/80 text-gray-600 hover:bg-gray-300 cursor-pointer"
        title="复制到剪贴板"
      >
        复制
      </button>
    </div>
  );
}
