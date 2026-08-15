import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { StatusChip } from './StatusChip';
import { SeverityBadge } from './SeverityBadge';
import {
  X,
  Cpu,
  Bot,
  AlertOctagon,
  Rocket,
  Activity,
  Layers,
  Copy,
  ExternalLink,
} from 'lucide-react';

export const InspectorDrawer: React.FC = () => {
  const { inspectorOpen, inspectorType, inspectorData, closeInspector, navigateTo } = useLoki();

  if (!inspectorOpen || !inspectorData) return null;

  return (
    <div
      id="inspector-drawer-backdrop"
      className="fixed inset-0 z-40 flex justify-end bg-[#010101]/60 backdrop-blur-xs transition-opacity"
      onClick={closeInspector}
    >
      <div
        id="inspector-drawer-panel"
        className="w-full max-w-lg h-full bg-[#050505] border-l border-[#23221C] shadow-2xl flex flex-col overflow-hidden animate-in slide-in-from-right duration-150"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div className="flex items-center justify-between px-5 py-4 bg-[#090909] border-b border-[#23221C] shrink-0">
          <div className="flex items-center gap-2.5">
            <div className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8AB55A]">
              {inspectorType === 'device' && <Cpu className="w-4 h-4" />}
              {inspectorType === 'agent' && <Bot className="w-4 h-4" />}
              {inspectorType === 'incident' && <AlertOctagon className="w-4 h-4" />}
              {inspectorType === 'deployment' && <Rocket className="w-4 h-4" />}
              {inspectorType === 'activity' && <Activity className="w-4 h-4" />}
              {inspectorType === 'rule' && <Layers className="w-4 h-4" />}
            </div>
            <div>
              <h3 className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                {inspectorType} Inspector
              </h3>
              <span className="text-sm font-bold text-[#E8D9A8] font-mono">
                {inspectorData.name || inspectorData.title || inspectorData.id || 'Details'}
              </span>
            </div>
          </div>

          <button
            onClick={closeInspector}
            className="p-1.5 rounded-[8px] text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08] transition-colors"
            aria-label="Close inspector"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Scrollable Content Body */}
        <div className="p-5 space-y-5 overflow-y-auto text-xs text-[#E8D9A8] flex-1">
          {/* Status & ID Summary */}
          <div className="p-3.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-[11px] text-[#8A8577] uppercase">Operational Status</span>
              {inspectorData.status ? (
                <StatusChip status={inspectorData.status} />
              ) : inspectorData.severity ? (
                <SeverityBadge severity={inspectorData.severity} />
              ) : (
                <span className="font-mono text-[#8AB55A]">Active</span>
              )}
            </div>
            {inspectorData.id && (
              <div className="flex items-center justify-between font-mono text-[11px]">
                <span className="text-[#8A8577]">Resource ID:</span>
                <span className="text-[#E8D9A8]">{inspectorData.id}</span>
              </div>
            )}
          </div>

          {/* Device specific fields */}
          {inspectorType === 'device' && (
            <div className="space-y-4 font-mono">
              <div>
                <span className="text-[11px] font-sans font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                  Hardware Identity & Interface
                </span>
                <div className="p-3 rounded bg-[#0A0A08] border border-[#23221C] space-y-1.5 text-xs">
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Family:</span>{' '}
                    <span>{inspectorData.family}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Manufacturer:</span>{' '}
                    <span>{inspectorData.manufacturer}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Hardware UID:</span>{' '}
                    <span className="text-[10px] break-all">{inspectorData.hardwareId}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Port / URI:</span>{' '}
                    <span>{inspectorData.transportPort}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Gateway:</span>{' '}
                    <span>{inspectorData.gatewayName}</span>
                  </div>
                </div>
              </div>

              <div>
                <span className="text-[11px] font-sans font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                  Firmware & Rollback
                </span>
                <div className="p-3 rounded bg-[#0A0A08] border border-[#23221C] space-y-1.5 text-xs">
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Active Version:</span>{' '}
                    <span className="text-[#8AB55A] font-semibold">
                      {inspectorData.firmwareVersion}
                    </span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Rollback Target:</span>{' '}
                    <span>{inspectorData.previousFirmwareVersion || 'None'}</span>
                  </div>
                  <div className="flex justify-between">
                    <span className="text-[#8A8577]">Rollback Status:</span>{' '}
                    <span>{inspectorData.rollbackEligible ? 'Eligible' : 'Unavailable'}</span>
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Activity / Incident diff or evidence */}
          {inspectorData.beforeAfterDiff && (
            <div>
              <span className="text-[11px] font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                Configuration Diff
              </span>
              <div className="p-3 rounded bg-[#0A0A08] border border-[#23221C] font-mono text-xs space-y-2">
                <div className="text-[#8A8577]">
                  Parameter:{' '}
                  <strong className="text-[#E8D9A8]">{inspectorData.beforeAfterDiff.key}</strong>
                </div>
                <div className="p-2 rounded bg-[#CC272E]/10 border border-[#CC272E]/30 text-[#CC272E]">
                  - {inspectorData.beforeAfterDiff.before}
                </div>
                <div className="p-2 rounded bg-[#6B8C42]/10 border border-[#6B8C42]/30 text-[#8AB55A]">
                  + {inspectorData.beforeAfterDiff.after}
                </div>
              </div>
            </div>
          )}

          {/* Full Raw JSON Payload */}
          <div>
            <div className="flex items-center justify-between mb-1.5">
              <span className="text-[11px] font-semibold text-[#8A8577] uppercase tracking-wider">
                Full Cryptographic & Data Contract Payload
              </span>
              <button
                onClick={() =>
                  navigator.clipboard.writeText(JSON.stringify(inspectorData, null, 2))
                }
                className="flex items-center gap-1 text-[11px] text-[#8A8577] hover:text-[#E8D9A8]"
              >
                <Copy className="w-3 h-3" /> Copy JSON
              </button>
            </div>
            <pre className="p-3 rounded-[8px] bg-[#010101] border border-[#23221C] font-mono text-[11px] text-[#E8D9A8] overflow-x-auto max-h-72">
              {JSON.stringify(inspectorData, null, 2)}
            </pre>
          </div>
        </div>

        {/* Footer Navigation Link */}
        <div className="px-5 py-3 bg-[#090909] border-t border-[#23221C] flex justify-end shrink-0">
          <button
            onClick={() => {
              if (inspectorType === 'device') navigateTo('devices', inspectorData.id);
              if (inspectorType === 'agent') navigateTo('agents', inspectorData.id);
              if (inspectorType === 'incident') navigateTo('incidents', inspectorData.id);
              if (inspectorType === 'deployment') navigateTo('deployments', inspectorData.id);
              if (inspectorType === 'activity') navigateTo('activity', inspectorData.id);
              closeInspector();
            }}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-semibold text-[#8AB55A] hover:bg-[#23221C] transition-colors"
          >
            Open Full Resource View <ExternalLink className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};
