import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { SeverityBadge } from './SeverityBadge';
import {
  ShieldAlert,
  Clock,
  Cpu,
  RotateCcw,
  CheckCircle2,
  XCircle,
  X,
  AlertTriangle,
  FileCode2,
} from 'lucide-react';

export const ApprovalModal: React.FC = () => {
  const { activeApprovalModal, setActiveApprovalModal, approveRequest, denyRequest } = useLoki();
  const [justification, setJustification] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  if (!activeApprovalModal) return null;

  const handleApprove = () => {
    setIsSubmitting(true);
    approveRequest(activeApprovalModal.id, justification);
    setIsSubmitting(false);
  };

  const handleDeny = () => {
    setIsSubmitting(true);
    denyRequest(activeApprovalModal.id, justification || 'Denied by administrator');
    setIsSubmitting(false);
  };

  return (
    <div
      id="approval-modal-backdrop"
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#010101]/80 backdrop-blur-sm p-4 overflow-y-auto"
      role="dialog"
      aria-modal="true"
      aria-labelledby="approval-modal-title"
    >
      <div
        id="approval-modal-card"
        className="relative w-full max-w-2xl bg-[#050505] border border-[#23221C] rounded-[14px] shadow-2xl overflow-hidden my-8"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-b border-[#23221C]">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-[8px] bg-[#23221C] text-[#E8D9A8]">
              <ShieldAlert className="w-5 h-5 text-[#8AB55A]" />
            </div>
            <div>
              <h2 id="approval-modal-title" className="text-base font-semibold text-[#E8D9A8]">
                Security Approval Required
              </h2>
              <p className="text-xs text-[#8A8577] font-mono">ID: {activeApprovalModal.id}</p>
            </div>
          </div>
          <button
            onClick={() => setActiveApprovalModal(null)}
            className="p-1.5 rounded-[8px] text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08] transition-colors"
            aria-label="Close dialog"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content Body */}
        <div className="p-6 space-y-5 text-sm max-h-[75vh] overflow-y-auto">
          {/* Top metadata grid */}
          <div className="grid grid-cols-2 sm:grid-cols-4 gap-3 p-3.5 rounded-[10px] bg-[#0A0A08] border border-[#23221C]">
            <div>
              <span className="text-[11px] text-[#8A8577] uppercase tracking-wider block mb-1">Requesting Actor</span>
              <span className="text-xs font-semibold text-[#E8D9A8] block">{activeApprovalModal.requestingActor.name}</span>
            </div>
            <div>
              <span className="text-[11px] text-[#8A8577] uppercase tracking-wider block mb-1">Risk Level</span>
              <SeverityBadge severity={activeApprovalModal.risk} />
            </div>
            <div>
              <span className="text-[11px] text-[#8A8577] uppercase tracking-wider block mb-1">Operation</span>
              <span className="text-xs font-mono font-medium text-[#8AB55A] block">{activeApprovalModal.operationName}</span>
            </div>
            <div>
              <span className="text-[11px] text-[#8A8577] uppercase tracking-wider block mb-1">Expiry Window</span>
              <span className="text-xs font-mono text-[#E8D9A8] flex items-center gap-1">
                <Clock className="w-3 h-3 text-[#8A8577]" />
                {activeApprovalModal.remainingMinutes}m remaining
              </span>
            </div>
          </div>

          {/* Target Devices */}
          <div>
            <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-2">
              Target Hardware Resource ({activeApprovalModal.targetDeviceIds.length} target)
            </span>
            <div className="flex flex-wrap gap-2">
              {activeApprovalModal.targetDeviceNames.map((name, idx) => (
                <div
                  key={idx}
                  className="flex items-center gap-2 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] font-mono text-xs text-[#E8D9A8]"
                >
                  <Cpu className="w-3.5 h-3.5 text-[#8AB55A]" />
                  <span>{name}</span>
                  <span className="text-[10px] text-[#8A8577]">({activeApprovalModal.targetDeviceIds[idx]})</span>
                </div>
              ))}
            </div>
          </div>

          {/* Operational Justification / Reason */}
          <div>
            <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
              Reason & Diagnosis
            </span>
            <p className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] leading-relaxed">
              {activeApprovalModal.reason}
            </p>
          </div>

          {/* Expected Effect & Hardware Downtime */}
          <div className="p-3 rounded-[8px] bg-[#23221C]/40 border border-[#23221C] flex gap-3 items-start">
            <AlertTriangle className="w-4 h-4 text-[#E8D9A8] shrink-0 mt-0.5" />
            <div>
              <span className="text-xs font-semibold text-[#E8D9A8] block mb-0.5">Expected Hardware Downtime</span>
              <p className="text-xs text-[#8A8577]">{activeApprovalModal.expectedEffectAndDowntime}</p>
            </div>
          </div>

          {/* Preflight Safety Results */}
          <div>
            <div className="flex items-center justify-between mb-2">
              <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider">
                Preflight Safety Check Assertions
              </span>
              <span className="text-xs font-mono text-[#8AB55A] flex items-center gap-1">
                <CheckCircle2 className="w-3.5 h-3.5" /> Passed
              </span>
            </div>
            <div className="space-y-1.5">
              {activeApprovalModal.preflightResult.checks.map((chk, i) => (
                <div
                  key={i}
                  className="flex items-center justify-between p-2.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs"
                >
                  <span className="text-[#E8D9A8]">{chk.name}</span>
                  <span className="text-[#8A8577] font-mono">{chk.details}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Operation Inputs / Parameters */}
          <div>
            <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
              Explicit Execution Parameters
            </span>
            <pre className="p-3 rounded-[8px] bg-[#010101] border border-[#23221C] font-mono text-xs text-[#E8D9A8] overflow-x-auto">
              {JSON.stringify(activeApprovalModal.inputs, null, 2)}
            </pre>
          </div>

          {/* Verification & Rollback Safeguards */}
          <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
            <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C]">
              <span className="text-xs font-semibold text-[#8AB55A] flex items-center gap-1.5 mb-1">
                <CheckCircle2 className="w-3.5 h-3.5" /> Post-Execution Verification
              </span>
              <p className="text-xs text-[#8A8577] leading-relaxed">{activeApprovalModal.verificationPlan}</p>
            </div>
            <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C]">
              <span className="text-xs font-semibold text-[#E8D9A8] flex items-center gap-1.5 mb-1">
                <RotateCcw className="w-3.5 h-3.5 text-[#8A8577]" /> Automated Rollback Plan
              </span>
              <p className="text-xs text-[#8A8577] leading-relaxed">{activeApprovalModal.rollbackPlan}</p>
            </div>
          </div>

          {/* Optional Approver Justification */}
          <div>
            <label htmlFor="approval-justification" className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
              Approver Audit Note (Optional)
            </label>
            <input
              id="approval-justification"
              type="text"
              value={justification}
              onChange={(e) => setJustification(e.target.value)}
              placeholder="e.g. Authorized hotfix after bench calibration review"
              className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
            />
          </div>
        </div>

        {/* Footer Actions */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-t border-[#23221C]">
          <button
            onClick={() => setActiveApprovalModal(null)}
            className="px-4 py-2 rounded-[8px] bg-transparent border border-[#23221C] text-xs font-medium text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08] transition-colors"
          >
            Cancel
          </button>

          <div className="flex items-center gap-3">
            <button
              onClick={handleDeny}
              disabled={isSubmitting}
              className="px-4 py-2 rounded-[8px] bg-[#CC272E]/10 border border-[#CC272E]/40 text-xs font-medium text-[#E8D9A8] hover:bg-[#CC272E]/20 transition-colors"
            >
              Deny Request
            </button>
            <button
              onClick={handleApprove}
              disabled={isSubmitting}
              className="px-5 py-2 rounded-[8px] bg-[#8AB55A] text-xs font-semibold text-[#010101] hover:bg-[#A8C878] transition-colors shadow-sm"
            >
              Approve & Dispatch
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
