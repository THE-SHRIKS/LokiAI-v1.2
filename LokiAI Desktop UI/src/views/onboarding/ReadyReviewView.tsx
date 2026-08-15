import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import {
  Sparkles,
  CheckCircle2,
  Briefcase,
  Bot,
  Cpu,
  Radio,
  ShieldCheck,
  ArrowRight,
  ArrowLeft,
  Settings,
  PlusCircle,
  AlertCircle,
  Clock,
  Layers,
} from 'lucide-react';

export const ReadyReviewView: React.FC = () => {
  const {
    userSession,
    onboardingState,
    finishOnboarding,
    navigateToRoute,
    setCurrentSettingsTab,
    addToast,
  } = useLoki();

  const [isOpening, setIsOpening] = useState(false);

  const hasDevice = !!onboardingState.discoveredDevice;

  const handleOpenLokiMain = () => {
    setIsOpening(true);
    addToast({
      type: 'success',
      title: 'Workspace Loaded',
      message: `Opened ${onboardingState.workspace.name} (${onboardingState.workspace.environment}).`,
    });

    setTimeout(() => {
      finishOnboarding();
    }, 500);
  };

  return (
    <div id="ready-review-screen" className="w-full max-w-4xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200">
      <OnboardingStepper currentStep="ready" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header Title */}
        <div className="text-center space-y-2">
          <div className="w-12 h-12 rounded-full bg-[#6B8C42]/20 border border-[#8AB55A] flex items-center justify-center text-[#8AB55A] mx-auto">
            <Sparkles className="w-6 h-6" />
          </div>
          <div>
            <span className="text-[10px] font-mono uppercase tracking-widest text-[#8AB55A]">
              Configuration Finalized
            </span>
            <h1 className="text-2xl font-bold text-[#E8D9A8] mt-1">LokiAI is ready</h1>
            <p className="text-xs text-[#8A8577] mt-1 max-w-md mx-auto">
              Your desktop hardware control room is configured with active safety gates, agent bridges, and runtime telemetry.
            </p>
          </div>
        </div>

        {/* Setup Summary Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3.5 text-xs">
          {/* Account Card */}
          <div className="p-4 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-2">
            <div className="flex items-center justify-between text-[#8A8577]">
              <span className="flex items-center gap-2 font-semibold text-[#E8D9A8]">
                <ShieldCheck className="w-4 h-4 text-[#8AB55A]" /> Authenticated Account
              </span>
              <span className="text-[10px] font-mono text-[#8AB55A]">Verified</span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C] flex items-center gap-2.5">
              <div className="w-8 h-8 rounded-full bg-[#23221C] flex items-center justify-center text-xs font-mono font-bold text-[#E8D9A8]">
                {userSession?.avatarInitials || 'LR'}
              </div>
              <div className="min-w-0">
                <div className="font-bold text-[#E8D9A8]">{userSession?.name || 'Laukik Rajput'}</div>
                <div className="text-[11px] text-[#8A8577] font-mono truncate">{userSession?.email || 'laukikxrajput@gmail.com'}</div>
              </div>
            </div>
          </div>

          {/* Workspace Card */}
          <div className="p-4 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-2">
            <div className="flex items-center justify-between text-[#8A8577]">
              <span className="flex items-center gap-2 font-semibold text-[#E8D9A8]">
                <Briefcase className="w-4 h-4 text-[#8AB55A]" /> Workspace Environment
              </span>
              <span className="text-[10px] font-mono uppercase text-[#8AB55A]">
                {onboardingState.workspace.environment}
              </span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
              <div className="font-bold text-[#E8D9A8] font-mono">{onboardingState.workspace.name}</div>
              <div className="text-[11px] text-[#8A8577] truncate mt-0.5">
                {onboardingState.workspace.description || 'Hardware validation environment'}
              </div>
            </div>
          </div>

          {/* Coding Agent Card */}
          <div className="p-4 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-2">
            <div className="flex items-center justify-between text-[#8A8577]">
              <span className="flex items-center gap-2 font-semibold text-[#E8D9A8]">
                <Bot className="w-4 h-4 text-[#8AB55A]" /> Coding Agent Bridge
              </span>
              <span className="text-[10px] font-mono text-[#8AB55A]">
                {onboardingState.connectedAgents.length} Active
              </span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C] space-y-1 font-mono text-[11px]">
              {onboardingState.connectedAgents.map((ag, i) => (
                <div key={i} className="flex items-center justify-between text-[#E8D9A8]">
                  <span className="flex items-center gap-1.5">
                    <span className="w-1.5 h-1.5 rounded-full bg-[#8AB55A]"></span>
                    {ag.name}
                  </span>
                  <span className="text-[10px] text-[#8A8577]">{ag.version}</span>
                </div>
              ))}
            </div>
          </div>

          {/* Hardware Device State Card */}
          <div className="p-4 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-2">
            <div className="flex items-center justify-between text-[#8A8577]">
              <span className="flex items-center gap-2 font-semibold text-[#E8D9A8]">
                <Cpu className="w-4 h-4 text-[#8AB55A]" /> Hardware & Health Status
              </span>
              <span className="text-[10px] font-mono text-[#8AB55A]">
                {hasDevice ? 'Validated' : 'Deferred'}
              </span>
            </div>
            <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
              {hasDevice ? (
                <div>
                  <div className="font-bold text-[#E8D9A8] font-mono flex items-center gap-2">
                    <span>{onboardingState.discoveredDevice?.name}</span>
                    <span className="text-[10px] px-1.5 rounded bg-[#23221C] text-[#8AB55A]">
                      {onboardingState.discoveredDevice?.family}
                    </span>
                  </div>
                  <div className="text-[11px] text-[#8A8577] font-mono mt-0.5">
                    Port: {onboardingState.discoveredDevice?.transportPort} • 7/7 Checks Passed
                  </div>
                </div>
              ) : (
                <div>
                  <div className="font-semibold text-[#E8D9A8]">Device will be connected later</div>
                  <div className="text-[11px] text-[#8A8577] font-mono mt-0.5">
                    Target: {onboardingState.scratchProject.targetFamily} ({onboardingState.scratchProject.intendedConnector})
                  </div>
                </div>
              )}
            </div>
          </div>
        </div>

        {/* Runtime & Security Parameters Strip */}
        <div className="p-3.5 rounded-[8px] bg-[#050505] border border-[#23221C] grid grid-cols-1 sm:grid-cols-3 gap-3 text-xs font-mono">
          <div>
            <span className="text-[10px] text-[#8A8577] uppercase block">Runtime State</span>
            <span className="text-[#8AB55A] font-semibold">127.0.0.1:3000 (Daemon OK)</span>
          </div>
          <div>
            <span className="text-[10px] text-[#8A8577] uppercase block">Permission Mode</span>
            <span className="text-[#E8D9A8]">Balanced (Approval Gates)</span>
          </div>
          <div>
            <span className="text-[10px] text-[#8A8577] uppercase block">Automatic Action Mode</span>
            <span className="text-[#8AB55A]">Standard Guardrails</span>
          </div>
        </div>

        {/* Primary Launch Action */}
        <div className="pt-2 space-y-3">
          <button
            id="open-loki-main-btn"
            type="button"
            onClick={handleOpenLokiMain}
            disabled={isOpening}
            className="w-full flex items-center justify-center gap-2 py-3.5 px-6 rounded-[8px] bg-[#8AB55A] hover:bg-[#A8C878] text-[#010101] font-bold text-sm transition-all shadow-lg active:scale-[0.99]"
          >
            {isOpening ? (
              <>
                <span className="w-4 h-4 border-2 border-[#010101] border-t-transparent rounded-full animate-spin"></span>
                <span>Initializing Dashboard...</span>
              </>
            ) : (
              <>
                <span>Open LokiAI Main</span>
                <ArrowRight className="w-4 h-4" />
              </>
            )}
          </button>
        </div>

        {/* Secondary Quick Adjustments */}
        <div className="pt-3 border-t border-[#23221C] flex flex-wrap items-center justify-between gap-3 text-xs text-[#8A8577]">
          <button
            type="button"
            onClick={() => {
              if (hasDevice) {
                navigateToRoute('onboarding/device-health');
              } else {
                navigateToRoute('onboarding/project');
              }
            }}
            className="hover:text-[#E8D9A8] flex items-center gap-1.5"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Return to previous step
          </button>

          <div className="flex items-center gap-4">
            <button
              type="button"
              onClick={() => navigateToRoute('onboarding/agent')}
              className="hover:text-[#8AB55A] flex items-center gap-1"
            >
              <PlusCircle className="w-3.5 h-3.5" /> Connect another agent
            </button>
            <button
              type="button"
              onClick={() => navigateToRoute('onboarding/device-connect')}
              className="hover:text-[#8AB55A] flex items-center gap-1"
            >
              <PlusCircle className="w-3.5 h-3.5" /> Add another device
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
