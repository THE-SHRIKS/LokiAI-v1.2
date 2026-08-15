import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import {
  AlertTriangle,
  LogOut,
  X,
  Radio,
  Rocket,
  Bot,
  CheckCircle2,
  AlertCircle,
} from 'lucide-react';

export const LogoutModal: React.FC = () => {
  const {
    isLogoutModalOpen,
    setIsLogoutModalOpen,
    confirmLogout,
    deployments,
    agentRuns,
    currentWorkspace,
    addToast,
  } = useLoki();

  const [isProcessing, setIsProcessing] = useState(false);
  const [errorScenario, setErrorScenario] = useState<string | null>(null);

  if (!isLogoutModalOpen) return null;

  const activeDeployments = deployments.filter(
    (d) => d.phase !== 'completed' && d.phase !== 'verified' && d.phase !== 'failed',
  );
  const activeAgentRuns = agentRuns.filter((a) => a.phase !== 'complete' && a.phase !== 'failed');

  const handleLogoutAction = (testFailureMode?: 'fail_api' | 'expired_session') => {
    setIsProcessing(true);
    setErrorScenario(null);

    setTimeout(() => {
      if (testFailureMode === 'fail_api') {
        setIsProcessing(false);
        setErrorScenario(
          'Cloud authentication service unreachable (HTTP 503). Local session can still be cleared safely.',
        );
        addToast({
          type: 'error',
          title: 'Remote Revocation Warning',
          message: 'Could not notify remote auth server. Local desktop session cleared.',
        });
        return;
      }

      if (testFailureMode === 'expired_session') {
        setIsProcessing(false);
        addToast({
          type: 'warning',
          title: 'Session Already Expired',
          message: 'Your token had expired. Returning to sign-in screen.',
        });
        confirmLogout();
        return;
      }

      setIsProcessing(false);
      confirmLogout();
    }, 600);
  };

  return (
    <div
      id="logout-confirmation-modal"
      className="fixed inset-0 bg-[#010101]/85 backdrop-blur-sm z-50 flex items-center justify-center p-4"
      role="dialog"
      aria-modal="true"
      aria-labelledby="logout-dialog-title"
    >
      <div className="w-full max-w-lg bg-[#0A0A08] border border-[#23221C] rounded-[12px] shadow-2xl overflow-hidden font-sans animate-in fade-in zoom-in-95 duration-150">
        {/* Modal Header */}
        <div className="px-6 py-4 bg-[#0F0F0C] border-b border-[#23221C] flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-[8px] bg-[#CC272E]/10 border border-[#CC272E]/30 flex items-center justify-center text-[#CC272E]">
              <LogOut className="w-4 h-4" />
            </div>
            <div>
              <h2 id="logout-dialog-title" className="text-sm font-bold text-[#E8D9A8]">
                Sign Out of LokiAI Main
              </h2>
              <span className="text-[11px] font-mono text-[#8A8577]">
                Desktop Runtime Session Disconnect
              </span>
            </div>
          </div>

          <button
            onClick={() => setIsLogoutModalOpen(false)}
            disabled={isProcessing}
            className="p-1 rounded-[6px] text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#23221C] transition-colors"
            aria-label="Close dialog"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Modal Body */}
        <div className="p-6 space-y-4 text-xs">
          {/* Main Informational Notice */}
          <div className="p-3.5 rounded-[8px] bg-[#050505] border border-[#23221C] text-[#E8D9A8] leading-relaxed">
            <p className="font-medium text-[#E8D9A8] mb-1">
              Signing out disconnects this desktop interface from your account.
            </p>
            <p className="text-[#8A8577] text-[11px]">
              It does <strong>not</strong> stop the LokiAI Runtime, interrupt active OTA
              deployments, disconnect hardware gateways, or terminate background agent tasks.
            </p>
          </div>

          {/* Running Tasks & Background Status */}
          <div className="space-y-2">
            <div className="text-[10px] font-mono uppercase tracking-wider text-[#8A8577] px-0.5">
              Live Background Services (Will Remain Running)
            </div>

            <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 text-[11px] font-mono">
              <div className="p-2.5 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-[#8A8577]">
                  <Radio className="w-3.5 h-3.5 text-[#8AB55A]" /> Local Daemon
                </span>
                <span className="text-[#8AB55A] font-bold">127.0.0.1:3000</span>
              </div>

              <div className="p-2.5 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-[#8A8577]">
                  <Rocket className="w-3.5 h-3.5 text-[#8AB55A]" /> OTA Jobs
                </span>
                <span className="text-[#E8D9A8]">
                  {activeDeployments.length > 0
                    ? `${activeDeployments.length} Active (Safe)`
                    : 'Idle'}
                </span>
              </div>

              <div className="p-2.5 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-[#8A8577]">
                  <Bot className="w-3.5 h-3.5 text-[#8AB55A]" /> Agent Sessions
                </span>
                <span className="text-[#E8D9A8]">
                  {activeAgentRuns.length > 0 ? `${activeAgentRuns.length} Running` : 'Preserved'}
                </span>
              </div>

              <div className="p-2.5 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <span className="flex items-center gap-1.5 text-[#8A8577]">
                  <CheckCircle2 className="w-3.5 h-3.5 text-[#8AB55A]" /> Workspace
                </span>
                <span className="text-[#8AB55A] truncate">{currentWorkspace.name}</span>
              </div>
            </div>
          </div>

          {/* Active Job Warning if any */}
          {activeDeployments.length > 0 && (
            <div className="p-2.5 rounded-[6px] bg-[#CC272E]/10 border border-[#CC272E]/30 flex items-start gap-2.5 text-[#E8D9A8]">
              <AlertTriangle className="w-4 h-4 text-[#CC272E] shrink-0 mt-0.5" />
              <div className="text-[11px] leading-tight">
                <span className="font-bold text-[#CC272E]">Active deployment in progress:</span>{' '}
                {activeDeployments[0].releaseTag}. Hardware update will complete in background
                daemon.
              </div>
            </div>
          )}

          {/* Error Message if simulated */}
          {errorScenario && (
            <div className="p-2.5 rounded-[6px] bg-[#CC272E]/10 border border-[#CC272E]/40 text-[#CC272E] text-xs">
              <div className="font-semibold flex items-center gap-1.5">
                <AlertCircle className="w-3.5 h-3.5" /> Notice
              </div>
              <p className="mt-1 text-[11px] text-[#E8D9A8]">{errorScenario}</p>
              <button
                onClick={() => confirmLogout()}
                className="mt-2 text-[10px] font-mono underline text-[#CC272E] hover:text-[#E8D9A8]"
              >
                Force Clear Local Desktop Session
              </button>
            </div>
          )}

          {/* Clear Scope Distinction */}
          <div className="text-[11px] text-[#8A8577] border-t border-[#23221C] pt-2">
            <span className="text-[#8AB55A] font-semibold">Scope of Action:</span> This will revoke
            the active desktop interface session token and return to the sign-in screen.
          </div>
        </div>

        {/* Modal Footer Actions */}
        <div className="px-6 py-4 bg-[#050505] border-t border-[#23221C] flex flex-wrap items-center justify-between gap-3">
          {/* Edge case test controls */}
          <div className="flex items-center gap-1 text-[10px] font-mono text-[#8A8577]">
            <span>Test Edge:</span>
            <button
              onClick={() => handleLogoutAction('fail_api')}
              className="px-1.5 py-0.5 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8A8577] hover:text-[#E8D9A8]"
              title="Simulate network failure when contacting remote auth"
            >
              Network Fail
            </button>
            <button
              onClick={() => handleLogoutAction('expired_session')}
              className="px-1.5 py-0.5 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8A8577] hover:text-[#E8D9A8]"
              title="Simulate token already expired"
            >
              Expired
            </button>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => setIsLogoutModalOpen(false)}
              disabled={isProcessing}
              className="px-4 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
            >
              Cancel
            </button>

            <button
              id="confirm-logout-button"
              onClick={() => handleLogoutAction()}
              disabled={isProcessing}
              className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#CC272E] text-[#010101] hover:bg-[#E83C44] text-xs font-bold transition-colors disabled:opacity-50"
            >
              {isProcessing ? (
                <>
                  <span className="w-3.5 h-3.5 border-2 border-[#010101] border-t-transparent rounded-full animate-spin"></span>
                  Signing Out...
                </>
              ) : (
                <>
                  <LogOut className="w-3.5 h-3.5" />
                  Log out
                </>
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
