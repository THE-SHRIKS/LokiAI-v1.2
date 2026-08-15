import React, { useState } from 'react';
import { useLoki } from '../context/LokiContext';
import { Incident } from '../types';
import { SeverityBadge } from '../components/common/SeverityBadge';
import { StatusChip } from '../components/common/StatusChip';
import {
  AlertOctagon,
  Search,
  Filter,
  ArrowLeft,
  ShieldCheck,
  CheckCircle2,
  Clock,
  FileCode,
  Terminal,
  Activity,
  Bot,
  ExternalLink,
  Layers,
} from 'lucide-react';

export const IncidentsView: React.FC = () => {
  const {
    incidents,
    selectedIncidentId,
    setSelectedIncidentId,
    resolveIncident,
    openInspector,
    navigateTo,
  } = useLoki();

  const [severityFilter, setSeverityFilter] = useState<string>('ALL');
  const [statusFilter, setStatusFilter] = useState<string>('ALL');
  const [resolutionNotes, setResolutionNotes] = useState('');
  const [isResolving, setIsResolving] = useState(false);

  const filteredIncidents = incidents.filter((inc) => {
    if (severityFilter !== 'ALL' && inc.severity !== severityFilter) return false;
    if (statusFilter !== 'ALL' && inc.status !== statusFilter) return false;
    return true;
  });

  const selectedIncident = incidents.find((i) => i.id === selectedIncidentId);

  // Incident Detail View
  if (selectedIncident) {
    const handleResolve = () => {
      resolveIncident(selectedIncident.id);
      setIsResolving(false);
    };

    return (
      <div id="incident-detail-view" className="space-y-6">
        {/* Header */}
        <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setSelectedIncidentId(null)}
              className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Back to incident list"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono font-bold text-sm text-[#8AB55A]">{selectedIncident.id}</span>
                <h2 className="text-base font-bold text-[#E8D9A8]">{selectedIncident.title}</h2>
                <SeverityBadge severity={selectedIncident.severity} />
                <StatusChip status={selectedIncident.status === 'Resolved' ? 'Resolved' : 'Running'} />
              </div>
              <p className="text-xs text-[#8A8577] font-mono mt-0.5">
                Target Node: <strong className="text-[#E8D9A8]">{selectedIncident.affectedDeviceName}</strong> • Opened: {selectedIncident.openedAt} • Assignee: {selectedIncident.assignedAgent || 'Unassigned'}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {selectedIncident.status !== 'Resolved' ? (
              <button
                onClick={() => setIsResolving(true)}
                disabled={!selectedIncident.verification.allPassed}
                className={`flex items-center gap-1.5 px-4 py-1.5 rounded-[8px] text-xs font-bold transition-colors ${
                  selectedIncident.verification.allPassed
                    ? 'bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878]'
                    : 'bg-[#23221C] text-[#8A8577] cursor-not-allowed'
                }`}
                title={!selectedIncident.verification.allPassed ? 'Cannot resolve until verification tests pass' : 'Mark incident resolved'}
              >
                <CheckCircle2 className="w-4 h-4" /> Resolve Incident
              </button>
            ) : (
              <span className="px-3 py-1.5 rounded-[8px] bg-[#6B8C42]/20 border border-[#8AB55A]/40 text-[#8AB55A] text-xs font-mono font-bold">
                ✓ Incident Resolved
              </span>
            )}

            <button
              onClick={() => openInspector('incident', selectedIncident)}
              className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#8AB55A] hover:bg-[#23221C] transition-colors"
            >
              Inspect Raw Evidence
            </button>
          </div>
        </div>

        {/* Resolve Modal / Form */}
        {isResolving && (
          <div className="p-4 rounded-[10px] bg-[#0A0A08] border border-[#8AB55A] space-y-3 font-mono text-xs animate-in fade-in">
            <div className="flex items-center justify-between">
              <span className="font-bold text-[#E8D9A8] font-sans">Confirm Incident Resolution</span>
              <span className="text-[11px] text-[#8AB55A]">Verification assertions satisfied</span>
            </div>
            <textarea
              value={resolutionNotes}
              onChange={(e) => setResolutionNotes(e.target.value)}
              placeholder="Enter resolution notes, root cause fix, or postmortem summary..."
              className="w-full h-20 p-2.5 rounded-[8px] bg-[#050505] border border-[#23221C] text-xs text-[#E8D9A8] placeholder-[#8A8577] focus:outline-none focus:border-[#8AB55A]"
            />
            <div className="flex justify-end gap-2">
              <button
                onClick={() => setIsResolving(false)}
                className="px-3 py-1.5 rounded-[6px] bg-transparent border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8]"
              >
                Cancel
              </button>
              <button
                onClick={handleResolve}
                className="px-4 py-1.5 rounded-[6px] bg-[#8AB55A] text-[#010101] font-bold hover:bg-[#A8C878]"
              >
                Submit Resolution
              </button>
            </div>
          </div>
        )}

        {/* 2-Column: Evidence Summary & Diagnosis */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Evidence Summary Card */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 font-mono text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C] font-sans">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Deterministic Evidence Package
              </span>
              <span className="text-[11px] text-[#8AB55A]">Captured at T0</span>
            </div>

            <div className="space-y-2">
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5 font-sans">Triggering Detection Rule</span>
                <span className="text-[#CC272E] font-bold">{selectedIncident.triggeringRule}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5 font-sans">Hardware Signal Excerpt</span>
                <span className="text-[#E8D9A8]">{selectedIncident.evidenceSnapshot?.rawSignalExcerpt}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5 font-sans">Active Firmware at Fault</span>
                <span className="text-[#8AB55A] font-bold">{selectedIncident.evidenceSnapshot?.firmwareVersionAtTrigger}</span>
              </div>
            </div>
          </div>

          {/* Investigation & Verification Panel */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Agent Investigation & Fix Status
              </span>
              <span className="text-[11px] font-mono text-[#8AB55A]">
                Confidence: {selectedIncident.investigation?.confidencePct || 95}%
              </span>
            </div>

            <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-1.5">
              <div className="flex items-center justify-between">
                <span className="font-semibold text-[#E8D9A8]">Assigned Agent</span>
                <span className="text-[11px] font-mono text-[#8AB55A]">{selectedIncident.assignedAgent || 'Unassigned'}</span>
              </div>
              <p className="text-[#8A8577] leading-relaxed font-sans">{selectedIncident.investigation?.diagnosisSummary}</p>
            </div>

            {/* Verification Check Status */}
            <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-2 font-mono">
              <div className="flex items-center justify-between font-sans">
                <span className="font-semibold text-[#E8D9A8]">Mandatory Post-Fix Verification</span>
                <span className={selectedIncident.verification?.allPassed ? 'text-[#8AB55A] font-bold' : 'text-[#E8D9A8]'}>
                  {selectedIncident.verification?.allPassed ? 'PASSED (60s stable)' : 'PENDING OBSERVATION'}
                </span>
              </div>
              <p className="text-[11px] text-[#8A8577]">
                Brownout watchdog, heap allocation integrity, and thermal thresholds must pass continuous 60-second observation before resolution can be finalized.
              </p>
            </div>
          </div>
        </div>

        {/* Chronological Incident Timeline */}
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 text-xs">
          <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
            <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
              Incident Lifecycle Events
            </span>
            <span className="text-[11px] font-mono text-[#8A8577]">{selectedIncident.timeline.length} events logged</span>
          </div>

          <div className="space-y-2.5">
            {selectedIncident.timeline.map((item) => (
              <div key={item.id} className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-1">
                <div className="flex items-center justify-between font-mono text-[11px]">
                  <span className="font-semibold text-[#8AB55A] uppercase">{item.type}</span>
                  <span className="text-[#8A8577]">{item.timestamp}</span>
                </div>
                <div className="font-semibold text-[#E8D9A8]">{item.title}</div>
                <p className="text-[#8A8577] leading-relaxed">{item.details}</p>
                <div className="text-[10px] text-[#8A8577] font-mono">Actor: {item.actor}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
    );
  }

  // Incidents List View
  return (
    <div id="incidents-view" className="space-y-6">
      {/* Filter Bar */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex items-center gap-3">
          <AlertOctagon className="w-4 h-4 text-[#8AB55A]" />
          <span className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
            Hardware Incidents & Evidence ({filteredIncidents.length})
          </span>
        </div>

        <div className="flex flex-wrap items-center gap-2 font-mono text-xs">
          <select
            value={severityFilter}
            onChange={(e) => setSeverityFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Severities</option>
            <option value="Critical">Critical</option>
            <option value="High">High</option>
            <option value="Medium">Medium</option>
            <option value="Low">Low</option>
          </select>

          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Statuses</option>
            <option value="Investigating">Investigating</option>
            <option value="Repair in Progress">Repair in Progress</option>
            <option value="Verifying">Verifying</option>
            <option value="Resolved">Resolved</option>
          </select>
        </div>
      </div>

      {/* Incidents Table */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="bg-[#090909] border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-3 px-4">Incident ID & Title</th>
                <th className="py-3 px-4">Severity</th>
                <th className="py-3 px-4">Lifecycle Status</th>
                <th className="py-3 px-4">Target Node</th>
                <th className="py-3 px-4">Triggering Rule</th>
                <th className="py-3 px-4">Opened</th>
                <th className="py-3 px-4 text-right">Inspect</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {filteredIncidents.map((inc) => (
                <tr
                  key={inc.id}
                  id={`incident-row-${inc.id}`}
                  onClick={() => setSelectedIncidentId(inc.id)}
                  className="hover:bg-[#0A0A08] cursor-pointer transition-colors group"
                >
                  <td className="py-3 px-4">
                    <div className="font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                      {inc.id}: {inc.title}
                    </div>
                    <div className="text-[10px] text-[#8A8577]">Assignee: {inc.assignedAgent || 'None'}</div>
                  </td>
                  <td className="py-3 px-4">
                    <SeverityBadge severity={inc.severity} />
                  </td>
                  <td className="py-3 px-4">
                    <StatusChip status={inc.status === 'Resolved' ? 'Resolved' : 'Running'} />
                  </td>
                  <td className="py-3 px-4 font-semibold text-[#8AB55A]">{inc.affectedDeviceName}</td>
                  <td className="py-3 px-4 text-[#8A8577]">{inc.triggeringRule}</td>
                  <td className="py-3 px-4 text-[#8A8577]">{inc.openedAt}</td>
                  <td className="py-3 px-4 text-right">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setSelectedIncidentId(inc.id);
                      }}
                      className="px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] text-[#8AB55A] hover:bg-[#23221C] transition-colors"
                    >
                      View Evidence
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
