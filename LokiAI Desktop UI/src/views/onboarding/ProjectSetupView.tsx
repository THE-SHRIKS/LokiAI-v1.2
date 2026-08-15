import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import {
  FolderGit2,
  Bot,
  Cpu,
  Layers,
  ArrowRight,
  ArrowLeft,
  CheckCircle2,
  ShieldCheck,
  Sparkles,
} from 'lucide-react';

export const ProjectSetupView: React.FC = () => {
  const { onboardingState, updateOnboardingState, navigateToRoute } = useLoki();

  const [repoPath, setRepoPath] = useState(
    onboardingState.scratchProject.repoPath || '~/projects/firmware-core'
  );
  const [targetFamily, setTargetFamily] = useState(
    onboardingState.scratchProject.targetFamily || 'ESP32-S3'
  );
  const [intendedConnector, setIntendedConnector] = useState(
    onboardingState.scratchProject.intendedConnector || 'USB-CDC Serial'
  );
  const [decidedLater, setDecidedLater] = useState(
    onboardingState.scratchProject.decidedLater || false
  );

  const handleContinue = () => {
    updateOnboardingState({
      discoveredDevice: null, // Device-free workspace
      scratchProject: {
        repoPath,
        targetFamily: decidedLater ? 'Undecided' : targetFamily,
        intendedConnector: decidedLater ? 'Deferred' : intendedConnector,
        decidedLater,
      },
      currentStep: 'ready',
    });

    navigateToRoute('onboarding/ready');
  };

  return (
    <div id="project-setup-screen" className="w-full max-w-3xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200">
      <OnboardingStepper currentStep="device" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header */}
        <div>
          <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
            <Sparkles className="w-3.5 h-3.5" /> Step 5: Project & Repository Setup
          </div>
          <h1 className="text-xl font-bold text-[#E8D9A8]">Prepare your project</h1>
          <p className="text-xs text-[#8A8577] mt-1.5 leading-relaxed">
            You can start building firmware and writing automated tests with your connected coding agent immediately. LokiAI will attach device discovery and health checks when hardware is ready.
          </p>
        </div>

        {/* Informative Chamber & Olive banner */}
        <div className="p-4 rounded-[8px] bg-[#050505] border border-[#6B8C42]/40 flex items-start gap-3 text-xs text-[#E8D9A8] leading-relaxed">
          <ShieldCheck className="w-5 h-5 text-[#8AB55A] shrink-0 mt-0.5" />
          <div>
            <strong className="text-[#8AB55A] block mb-0.5">Device-Free Workspace Initialization</strong>
            Your workspace will be initialized with full operational tools (agent sandboxing, simulated telemetry, CI/CD firmware builds, and approval workflows). Hardware links can be attached anytime via the TopBar.
          </div>
        </div>

        {/* Active Context Badges */}
        <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs font-mono">
          <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
            <span className="text-[10px] text-[#8A8577] uppercase block mb-1">Active Workspace</span>
            <span className="font-bold text-[#E8D9A8]">{onboardingState.workspace.name}</span>
            <span className="text-[10px] text-[#8AB55A] ml-2">({onboardingState.workspace.environment})</span>
          </div>

          <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
            <span className="text-[10px] text-[#8A8577] uppercase block mb-1">Connected Agent</span>
            <span className="font-bold text-[#E8D9A8]">
              {onboardingState.connectedAgents[0]?.name || 'Claude Code'}
            </span>
          </div>
        </div>

        {/* Project Path & Target Family Form */}
        <div className="space-y-4 text-xs">
          {/* Project Repository Path */}
          <div>
            <label className="block text-xs font-semibold text-[#E8D9A8] mb-1.5">
              Firmware Codebase Directory Path
            </label>
            <div className="relative">
              <input
                type="text"
                value={repoPath}
                onChange={(e) => setRepoPath(e.target.value)}
                placeholder="~/projects/firmware-core"
                className="w-full px-3.5 py-2.5 rounded-[8px] bg-[#050505] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
              />
            </div>
            <span className="text-[10px] text-[#8A8577] font-mono mt-1 block">
              Agents use this workspace directory to read manifests, build binaries, and analyze compilation outputs.
            </span>
          </div>

          {/* Optional Target Hardware Family */}
          <div className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-3">
            <div className="flex items-center justify-between">
              <span className="font-semibold text-[#E8D9A8]">Target Hardware Architecture</span>
              <label className="flex items-center gap-1.5 text-[11px] text-[#8A8577] cursor-pointer">
                <input
                  type="checkbox"
                  checked={decidedLater}
                  onChange={(e) => setDecidedLater(e.target.checked)}
                  className="rounded border-[#23221C] text-[#8AB55A] focus:ring-0"
                />
                <span>I haven’t decided yet</span>
              </label>
            </div>

            {!decidedLater ? (
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div>
                  <label className="block text-[11px] text-[#8A8577] mb-1">Target Silicon Family</label>
                  <select
                    value={targetFamily}
                    onChange={(e) => setTargetFamily(e.target.value)}
                    className="w-full p-2.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] text-xs font-mono"
                  >
                    <option value="ESP32-S3">ESP32-S3 (Xtensa LX7 Dual Core)</option>
                    <option value="STM32-H7">STM32-H7 (ARM Cortex-M7 480MHz)</option>
                    <option value="nRF5340">nRF5340 (Dual ARM Cortex-M33 BLE)</option>
                    <option value="Raspberry Pi CM4">Raspberry Pi CM4 (BCM2711 Linux)</option>
                    <option value="i.MX8M-Plus">i.MX8M-Plus (NXP Quad A53)</option>
                  </select>
                </div>

                <div>
                  <label className="block text-[11px] text-[#8A8577] mb-1">Intended Deployment Method</label>
                  <select
                    value={intendedConnector}
                    onChange={(e) => setIntendedConnector(e.target.value)}
                    className="w-full p-2.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] text-xs font-mono"
                  >
                    <option value="USB-CDC Serial">USB-CDC Serial Bootloader</option>
                    <option value="SWD / JTAG Probe">SWD / JTAG Hardware Probe</option>
                    <option value="Industrial Ethernet / MQTT">Industrial Ethernet / OTA</option>
                    <option value="BLE Gateway (mTLS)">BLE Mesh Gateway</option>
                  </select>
                </div>
              </div>
            ) : (
              <p className="text-[11px] text-[#8A8577] font-mono">
                Hardware family will be detected automatically when you attach a device later.
              </p>
            )}
          </div>
        </div>

        {/* Footer Navigation */}
        <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
          <button
            type="button"
            onClick={() => navigateToRoute('onboarding/device-choice')}
            className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Back
          </button>

          <button
            id="continue-scratch-btn"
            type="button"
            onClick={handleContinue}
            className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors shadow-sm"
          >
            <span>Continue to Review</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};
