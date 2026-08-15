import React, { useState } from 'react';
import { useLoki } from '../context/LokiContext';
import { TelemetryChart } from '../components/common/TelemetryChart';
import { LogViewer } from '../components/common/LogViewer';
import { SeverityBadge } from '../components/common/SeverityBadge';
import {
  Activity,
  Radio,
  Sliders,
  CheckCircle2,
  AlertTriangle,
  Play,
  Pause,
  PlusCircle,
  Clock,
  Layers,
  Terminal,
} from 'lucide-react';

export const MonitoringView: React.FC = () => {
  const { watchers, monitoringRules, toggleRule, openInspector, logs } = useLoki();
  const [selectedWatcherId, setSelectedWatcherId] = useState<string>(watchers[0]?.id || '');

  return (
    <div id="monitoring-view" className="space-y-6">
      {/* 1st Row: Monitoring Coverage & Gateway Metrics */}
      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
        {/* Coverage Card */}
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
            <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
              Fleet Telemetry Coverage
            </span>
            <span className="text-[11px] font-mono text-[#8AB55A]">100% Protected</span>
          </div>

          <div className="grid grid-cols-3 gap-2 text-center font-mono">
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
              <span className="text-lg font-bold text-[#8AB55A]">5</span>
              <span className="text-[10px] text-[#8A8577] block mt-0.5">Continuous</span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
              <span className="text-lg font-bold text-[#E8D9A8]">1</span>
              <span className="text-[10px] text-[#8A8577] block mt-0.5">Sampled</span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
              <span className="text-lg font-bold text-[#8A8577]">0</span>
              <span className="text-[10px] text-[#8A8577] block mt-0.5">Offline</span>
            </div>
          </div>

          <p className="text-xs text-[#8A8577] leading-relaxed">
            All 6 runtime hardware nodes emit 1Hz heartbeat telemetry with brownout and watchdog tracking.
          </p>
        </div>

        {/* Watcher Gateways Summary */}
        <div className="lg:col-span-2 bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3">
          <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
            <div className="flex items-center gap-2">
              <Radio className="w-4 h-4 text-[#8AB55A]" />
              <span className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
                Watcher & Hardware Interface Gateways
              </span>
            </div>
            <span className="text-[11px] font-mono text-[#8A8577]">{watchers.length} Active Gateways</span>
          </div>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
            {watchers.map((w) => (
              <div
                key={w.id}
                className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-1.5 font-mono text-xs"
              >
                <div className="flex items-center justify-between">
                  <span className="font-bold text-[#E8D9A8]">{w.name}</span>
                  <span className="w-2 h-2 rounded-full bg-[#8AB55A]"></span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>Host / Host Gateway:</span> <span className="text-[#E8D9A8]">{w.host}</span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>Nodes:</span> <span className="text-[#8AB55A]">{w.connectedDevicesCount} attached</span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>Latency / Drop:</span> <span>{w.latencyMs}ms • {w.dropRatePerSec}/s drops</span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* 2nd Row: Full Interactive Telemetry Chart */}
      <TelemetryChart title="Live Chamber Hardware Signals & Multi-Rail Telemetry" />

      {/* 3rd Row: Rule Engine */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-4">
        <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-[#23221C]">
          <div className="flex items-center gap-2">
            <Sliders className="w-4 h-4 text-[#8AB55A]" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
              Automated Incident Rule Engine ({monitoringRules.length} rules)
            </h3>
          </div>

          <span className="text-[11px] font-mono text-[#8A8577]">
            Evaluates stream metrics with sliding windows
          </span>
        </div>

        <div className="space-y-2.5">
          {monitoringRules.map((rule) => (
            <div
              key={rule.id}
              className="p-3.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex flex-wrap items-center justify-between gap-3 font-mono text-xs group hover:border-[#8AB55A]/30 transition-all"
            >
              <div className="space-y-1 min-w-0">
                <div className="flex items-center gap-2">
                  <span className="font-bold text-[#E8D9A8] font-sans">{rule.name}</span>
                  <SeverityBadge severity={rule.severity} />
                  <span className="text-[10px] px-1.5 py-0.2 rounded bg-[#23221C] text-[#8A8577]">
                    Window: {rule.windowSeconds}s
                  </span>
                </div>
                <div className="text-[11px] text-[#8A8577] font-mono">
                  Condition: <strong className="text-[#8AB55A]">{rule.condition}</strong>
                </div>
              </div>

              <div className="flex items-center gap-3">
                <button
                  onClick={() => openInspector('rule', rule)}
                  className="text-[11px] text-[#8A8577] hover:text-[#E8D9A8] hover:underline"
                >
                  Inspect Rule
                </button>

                <button
                  onClick={() => toggleRule(rule.id)}
                  className={`px-3 py-1 rounded-[6px] text-xs font-medium transition-all ${
                    rule.enabled
                      ? 'bg-[#6B8C42]/20 border border-[#8AB55A]/40 text-[#8AB55A]'
                      : 'bg-[#23221C] text-[#8A8577]'
                  }`}
                >
                  {rule.enabled ? 'Enabled' : 'Disabled'}
                </button>
              </div>
            </div>
          ))}
        </div>
      </div>

      {/* 4th Row: Bounded Operational Logs */}
      <LogViewer logs={logs} title="Fleet-Wide Operational Telemetry & Watcher Stream Logs" />
    </div>
  );
};
