import React, { useState, useMemo } from 'react';
import { LogEntry } from '../../types';
import { Terminal, Search, Filter, Copy, Check, Pause, Play } from 'lucide-react';

interface LogViewerProps {
  logs: LogEntry[];
  title?: string;
  sourceFilter?: string;
  maxHeight?: string;
  className?: string;
}

export const LogViewer: React.FC<LogViewerProps> = ({
  logs,
  title = 'Bounded Operational Logs',
  sourceFilter,
  maxHeight = 'max-h-72',
  className = '',
}) => {
  const [query, setQuery] = useState('');
  const [levelFilter, setLevelFilter] = useState<'ALL' | 'INFO' | 'WARN' | 'ERROR' | 'FATAL'>('ALL');
  const [isPaused, setIsPaused] = useState(false);
  const [copied, setCopied] = useState(false);

  const filteredLogs = useMemo(() => {
    return logs.filter((log) => {
      if (sourceFilter && log.sourceDevice !== sourceFilter && log.service !== sourceFilter) {
        return false;
      }
      if (levelFilter !== 'ALL' && log.level !== levelFilter) {
        return false;
      }
      if (query.trim()) {
        const q = query.toLowerCase();
        return (
          log.message.toLowerCase().includes(q) ||
          log.sourceDevice.toLowerCase().includes(q) ||
          log.service.toLowerCase().includes(q) ||
          (log.correlationIncidentId && log.correlationIncidentId.toLowerCase().includes(q)) ||
          (log.correlationJobId && log.correlationJobId.toLowerCase().includes(q))
        );
      }
      return true;
    });
  }, [logs, sourceFilter, levelFilter, query]);

  const handleCopy = () => {
    const text = filteredLogs.map((l) => `${l.timestamp} [${l.level}] ${l.sourceDevice} (${l.service}): ${l.message}`).join('\n');
    navigator.clipboard.writeText(text);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const getLevelColor = (lvl: string) => {
    switch (lvl) {
      case 'FATAL':
      case 'ERROR':
        return 'text-[#CC272E] font-bold';
      case 'WARN':
        return 'text-[#E8D9A8] font-semibold';
      case 'INFO':
        return 'text-[#8AB55A]';
      case 'DEBUG':
      default:
        return 'text-[#8A8577]';
    }
  };

  return (
    <div id="log-viewer-container" className={`bg-[#050505] border border-[#23221C] rounded-[10px] p-4 ${className}`}>
      {/* Top Controls */}
      <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-[#23221C] mb-3">
        <div className="flex items-center gap-2">
          <Terminal className="w-4 h-4 text-[#8AB55A]" />
          <h4 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">{title}</h4>
          <span className="text-[11px] font-mono text-[#8A8577]">({filteredLogs.length} events)</span>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Level Filter */}
          <div className="flex items-center rounded-[6px] bg-[#0A0A08] border border-[#23221C] p-0.5 font-mono text-[11px]">
            {(['ALL', 'INFO', 'WARN', 'ERROR', 'FATAL'] as const).map((lvl) => (
              <button
                key={lvl}
                onClick={() => setLevelFilter(lvl)}
                className={`px-2 py-0.5 rounded-[4px] transition-all ${
                  levelFilter === lvl
                    ? 'bg-[#23221C] text-[#E8D9A8] font-semibold'
                    : 'text-[#8A8577] hover:text-[#E8D9A8]'
                }`}
              >
                {lvl}
              </button>
            ))}
          </div>

          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-[#8A8577] absolute left-2 top-2" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter logs..."
              className="pl-7 pr-2 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] placeholder-[#8A8577] focus:outline-none focus:border-[#8AB55A] w-36 sm:w-48 font-mono"
            />
          </div>

          {/* Pause / Resume Streaming */}
          <button
            onClick={() => setIsPaused(!isPaused)}
            className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            title={isPaused ? 'Resume streaming' : 'Pause visual stream'}
            aria-label="Toggle log pause"
          >
            {isPaused ? <Play className="w-3.5 h-3.5 text-[#8AB55A]" /> : <Pause className="w-3.5 h-3.5" />}
          </button>

          {/* Copy Button */}
          <button
            onClick={handleCopy}
            className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            title="Copy logs to clipboard"
            aria-label="Copy logs"
          >
            {copied ? <Check className="w-3.5 h-3.5 text-[#8AB55A]" /> : <Copy className="w-3.5 h-3.5" />}
          </button>
        </div>
      </div>

      {/* Log Console Body */}
      <div
        className={`bg-[#010101] border border-[#23221C] rounded-[8px] p-3 font-mono text-xs overflow-y-auto space-y-1.5 ${maxHeight} text-[#E8D9A8]`}
        role="log"
        aria-live="polite"
      >
        {filteredLogs.length === 0 ? (
          <div className="text-center py-6 text-xs text-[#8A8577]">
            No log entries matching filter conditions.
          </div>
        ) : (
          filteredLogs.map((log) => (
            <div key={log.id} className="flex items-start gap-2 hover:bg-[#0A0A08] p-1 rounded transition-colors group">
              <span className="text-[#8A8577] text-[11px] shrink-0">{log.timestamp}</span>
              <span className={`px-1 rounded bg-[#0A0A08] border border-[#23221C] text-[10px] shrink-0 ${getLevelColor(log.level)}`}>
                {log.level}
              </span>
              <span className="text-[#8A8577] text-[11px] shrink-0">[{log.sourceDevice}]</span>
              <span className="text-[#8AB55A] text-[11px] shrink-0">{log.service}:</span>
              <span className="text-[#E8D9A8] break-all">{log.message}</span>
              {log.correlationIncidentId && (
                <span className="ml-auto text-[10px] px-1.5 py-0.2 rounded bg-[#23221C] text-[#E8D9A8] shrink-0">
                  {log.correlationIncidentId}
                </span>
              )}
            </div>
          ))
        )}
      </div>
    </div>
  );
};
