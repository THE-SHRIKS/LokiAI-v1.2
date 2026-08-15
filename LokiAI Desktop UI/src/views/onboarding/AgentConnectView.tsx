import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import {
  Bot,
  CheckCircle2,
  AlertTriangle,
  ArrowRight,
  ArrowLeft,
  ShieldAlert,
  ShieldCheck,
  RefreshCw,
  Copy,
  Check,
} from 'lucide-react';

interface AgentProviderDefinition {
  id: 'claude' | 'codex' | 'cursor' | 'gravity';
  displayName: string;
  cliTool: string;
  defaultModel: string;
  defaultVersion: string;
  detected: boolean;
  detectedPath?: string;
  commandSnippet: string;
  capabilities: string[];
  protocol: 'MCP' | 'CLI Bridge' | 'Loki Daemon ACP';
}

const providerDefinitions: AgentProviderDefinition[] = [
  {
    id: 'claude',
    displayName: 'Claude Code',
    cliTool: 'claude',
    defaultModel: 'claude-3-7-sonnet',
    defaultVersion: 'v2.4.1 (Detected)',
    detected: true,
    detectedPath: '/usr/local/bin/claude',
    commandSnippet: 'claude mcp add lokiai -- http://127.0.0.1:3000/mcp',
    capabilities: [
      'Read Real-time Telemetry & Logs',
      'Trigger Diagnostic Pre-flights',
      'Investigate Hardware Incidents',
      'OTA Deployment (Requires Human Approval)',
    ],
    protocol: 'MCP',
  },
  {
    id: 'codex',
    displayName: 'Codex / ChatGPT',
    cliTool: 'codex-hardware-cli',
    defaultModel: 'gpt-4o-hardware-specialist',
    defaultVersion: 'v1.9.0',
    detected: true,
    detectedPath: '~/.loki/bin/codex-bridge',
    commandSnippet: 'codex connect lokiai --endpoint http://127.0.0.1:3000',
    capabilities: [
      'Read System Health & Rules',
      'Execute Non-destructive Self Tests',
      'Generate Firmware Patches',
    ],
    protocol: 'CLI Bridge',
  },
  {
    id: 'cursor',
    displayName: 'Cursor',
    cliTool: 'cursor',
    defaultModel: 'cursor-agent-v2',
    defaultVersion: 'v0.45.2',
    detected: true,
    detectedPath: '/Applications/Cursor.app/Contents/MacOS/Cursor',
    commandSnippet: 'cursor --connect-mcp http://127.0.0.1:3000/mcp',
    capabilities: [
      'Read Active Incidents & Log Streams',
      'Trigger Local Test Suites',
      'Review Historical Deployments',
    ],
    protocol: 'MCP',
  },
  {
    id: 'gravity',
    displayName: 'Gravity Agent',
    cliTool: 'gravity-runtime',
    defaultModel: 'gravity-embedded-v3',
    defaultVersion: 'v3.1.0-native',
    detected: false,
    commandSnippet: 'gravity attach --daemon http://127.0.0.1:3000',
    capabilities: [
      'Autonomous Telemetry Monitoring',
      'Automatic Rollback on Assertion Failure',
      'Hardware Topology Inspection',
    ],
    protocol: 'Loki Daemon ACP',
  },
];

export const AgentConnectView: React.FC = () => {
  const { onboardingState, updateOnboardingState, navigateToRoute, addToast } = useLoki();

  const [selectedProviderId, setSelectedProviderId] = useState<string>('claude');
  const [copiedSnippet, setCopiedSnippet] = useState(false);
  const [isConnecting, setIsConnecting] = useState(false);
  const [connectError, setConnectError] = useState<string | null>(null);

  const selectedDef =
    providerDefinitions.find((p) => p.id === selectedProviderId) || providerDefinitions[0];

  const isConnected = (providerName: string) => {
    return onboardingState.connectedAgents.some(
      (a) =>
        a.name.toLowerCase().includes(providerName.toLowerCase()) ||
        a.provider.toLowerCase().includes(providerName.toLowerCase()),
    );
  };

  const handleCopyCommand = () => {
    navigator.clipboard.writeText(selectedDef.commandSnippet);
    setCopiedSnippet(true);
    setTimeout(() => setCopiedSnippet(false), 2000);
    addToast({
      type: 'info',
      title: 'Command Copied',
      message: 'Paste into your terminal to pair the coding agent.',
    });
  };

  const handleConnectSelectedAgent = () => {
    setIsConnecting(true);
    setConnectError(null);

    setTimeout(() => {
      setIsConnecting(false);

      if (selectedDef.id === 'gravity' && !selectedDef.detected) {
        setConnectError(
          'Gravity runtime binary not detected at standard path. Ensure daemon is installed or use Claude Code/Codex.',
        );
        return;
      }

      const newAgentEntry: any = {
        provider: selectedDef.displayName,
        name: `${selectedDef.displayName} (${selectedDef.protocol})`,
        model: selectedDef.defaultModel,
        version: selectedDef.defaultVersion,
        status: 'connected',
        capabilities: selectedDef.capabilities,
        lastHandshake: 'Just now',
      };

      const existingFiltered = onboardingState.connectedAgents.filter(
        (a) => !a.name.includes(selectedDef.displayName),
      );

      updateOnboardingState({
        connectedAgents: [...existingFiltered, newAgentEntry],
      });

      addToast({
        type: 'success',
        title: 'Coding Agent Connected',
        message: `Paired ${selectedDef.displayName} with restricted hardware capability scope.`,
      });
    }, 900);
  };

  const handleContinue = () => {
    if (onboardingState.connectedAgents.length === 0) {
      addToast({
        type: 'warning',
        title: 'Agent Required',
        message: 'Please connect at least one coding agent before proceeding.',
      });
      return;
    }
    updateOnboardingState({ currentStep: 'device' });
    navigateToRoute('onboarding/device-choice');
  };

  return (
    <div
      id="agent-connect-screen"
      className="w-full max-w-4xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200"
    >
      <OnboardingStepper currentStep="agent" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header */}
        <div>
          <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
            <Bot className="w-3.5 h-3.5" /> Step 3: Coding Agent Integration
          </div>
          <h1 className="text-xl font-bold text-[#E8D9A8]">Connect your coding agent</h1>
          <p className="text-xs text-[#8A8577] mt-1.5 leading-relaxed">
            LokiAI gives your coding agent controlled access to device health, incidents, builds,
            deployments, tests, and verification.
          </p>
        </div>

        {/* 2-Column: Provider Selection Cards on Left, Configuration Panel on Right */}
        <div className="grid grid-cols-1 lg:grid-cols-12 gap-6">
          {/* Provider Cards Column */}
          <div className="lg:col-span-5 space-y-2.5">
            <div className="text-[11px] font-semibold uppercase font-mono text-[#8A8577] px-0.5">
              Available Agent Bridges
            </div>

            {providerDefinitions.map((provider) => {
              const alreadyConnected = isConnected(provider.displayName);
              const isSelected = selectedProviderId === provider.id;

              return (
                <button
                  key={provider.id}
                  type="button"
                  onClick={() => {
                    setSelectedProviderId(provider.id);
                    setConnectError(null);
                  }}
                  className={`w-full p-3.5 rounded-[10px] border text-left transition-all relative ${
                    isSelected
                      ? 'bg-[#6B8C42]/10 border-[#8AB55A] shadow-md'
                      : 'bg-[#050505] border-[#23221C] hover:border-[#8AB55A]/40'
                  }`}
                >
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2.5">
                      <div
                        className={`w-8 h-8 rounded-[6px] flex items-center justify-center font-bold text-xs ${
                          alreadyConnected
                            ? 'bg-[#6B8C42]/20 text-[#8AB55A] border border-[#8AB55A]'
                            : 'bg-[#23221C] text-[#E8D9A8]'
                        }`}
                      >
                        <Bot className="w-4 h-4" />
                      </div>
                      <div>
                        <div className="font-bold text-xs text-[#E8D9A8]">
                          {provider.displayName}
                        </div>
                        <div className="text-[10px] text-[#8A8577] font-mono">
                          {provider.defaultModel}
                        </div>
                      </div>
                    </div>

                    {alreadyConnected ? (
                      <span className="flex items-center gap-1 text-[10px] font-mono font-bold text-[#8AB55A] px-2 py-0.5 rounded bg-[#6B8C42]/20 border border-[#8AB55A]/40">
                        <Check className="w-3 h-3" /> Connected
                      </span>
                    ) : provider.detected ? (
                      <span className="text-[10px] font-mono text-[#8AB55A] px-1.5 py-0.5 rounded bg-[#0A0A08] border border-[#23221C]">
                        Detected
                      </span>
                    ) : (
                      <span className="text-[10px] font-mono text-[#8A8577] px-1.5 py-0.5 rounded bg-[#0A0A08] border border-[#23221C]">
                        Config required
                      </span>
                    )}
                  </div>
                </button>
              );
            })}

            {/* Note on scoped security */}
            <div className="p-3 rounded-[8px] bg-[#050505] border border-[#23221C] text-[11px] text-[#8A8577] space-y-1">
              <div className="font-semibold text-[#E8D9A8] flex items-center gap-1.5">
                <ShieldCheck className="w-3.5 h-3.5 text-[#8AB55A]" /> Zero-Unchecked Hardware
                Access
              </div>
              <p>
                LokiAI strictly enforces that coding agents cannot bypass approval gates for
                physical resets or flash writes.
              </p>
            </div>
          </div>

          {/* Configuration / Detail Setup Panel on Right */}
          <div className="lg:col-span-7 p-5 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-[#23221C]">
              <div>
                <h3 className="text-sm font-bold text-[#E8D9A8]">
                  {selectedDef.displayName} Configuration
                </h3>
                <span className="text-[11px] font-mono text-[#8A8577]">
                  Protocol: <strong className="text-[#8AB55A]">{selectedDef.protocol}</strong>
                </span>
              </div>
              {selectedDef.detected && (
                <span className="text-[10px] font-mono text-[#8AB55A] bg-[#6B8C42]/20 border border-[#8AB55A]/30 px-2 py-0.5 rounded">
                  {selectedDef.defaultVersion}
                </span>
              )}
            </div>

            {/* Terminal Command Snippet */}
            <div className="space-y-1.5">
              <div className="flex items-center justify-between text-xs text-[#8A8577]">
                <span>1. Connect via CLI or MCP Configuration</span>
                <button
                  type="button"
                  onClick={handleCopyCommand}
                  className="flex items-center gap-1 text-[11px] font-mono text-[#8AB55A] hover:underline"
                >
                  {copiedSnippet ? <Check className="w-3 h-3" /> : <Copy className="w-3 h-3" />}
                  {copiedSnippet ? 'Copied' : 'Copy command'}
                </button>
              </div>

              <div className="p-3 rounded-[6px] bg-[#010101] border border-[#23221C] font-mono text-xs text-[#8AB55A] flex items-center justify-between gap-2 overflow-x-auto">
                <code>{selectedDef.commandSnippet}</code>
              </div>
            </div>

            {/* Requested Capabilities */}
            <div className="space-y-2">
              <span className="text-xs font-semibold text-[#E8D9A8] block">
                2. Requested Scoped Capabilities
              </span>
              <div className="space-y-1.5">
                {selectedDef.capabilities.map((cap, idx) => (
                  <div
                    key={idx}
                    className="p-2 rounded bg-[#0A0A08] border border-[#23221C] flex items-center gap-2 text-xs text-[#E8D9A8]"
                  >
                    <CheckCircle2 className="w-3.5 h-3.5 text-[#8AB55A] shrink-0" />
                    <span>{cap}</span>
                  </div>
                ))}
              </div>
            </div>

            {/* Security Explanation */}
            <div className="p-3 rounded bg-[#0A0A08] border border-[#23221C] flex items-start gap-2.5 text-[11px] text-[#8A8577]">
              <ShieldAlert className="w-4 h-4 text-[#8AB55A] shrink-0 mt-0.5" />
              <span>
                Dangerous operations (flash writes, power overrides, rollback triggers) will
                generate a cryptographic approval prompt in your TopBar.
              </span>
            </div>

            {/* Error Message if failed */}
            {connectError && (
              <div className="p-2.5 rounded bg-[#CC272E]/10 border border-[#CC272E]/40 text-xs text-[#CC272E] flex items-center gap-2 font-mono">
                <AlertTriangle className="w-4 h-4 shrink-0" />
                <span>{connectError}</span>
              </div>
            )}

            {/* Connect Agent CTA */}
            <div className="pt-2">
              <button
                id="connect-agent-btn"
                type="button"
                onClick={handleConnectSelectedAgent}
                disabled={isConnecting}
                className="w-full flex items-center justify-center gap-2 py-2.5 px-4 rounded-[8px] bg-[#23221C] hover:bg-[#38362D] text-[#E8D9A8] border border-[#8AB55A]/50 hover:border-[#8AB55A] text-xs font-bold transition-all"
              >
                {isConnecting ? (
                  <>
                    <RefreshCw className="w-3.5 h-3.5 animate-spin text-[#8AB55A]" />
                    <span>Performing Protocol Handshake...</span>
                  </>
                ) : isConnected(selectedDef.displayName) ? (
                  <>
                    <Check className="w-3.5 h-3.5 text-[#8AB55A]" />
                    <span>Re-verify Connection ({selectedDef.displayName})</span>
                  </>
                ) : (
                  <>
                    <Bot className="w-3.5 h-3.5 text-[#8AB55A]" />
                    <span>Connect & Verify {selectedDef.displayName}</span>
                  </>
                )}
              </button>
            </div>
          </div>
        </div>

        {/* Connected Agents Summary Section */}
        {onboardingState.connectedAgents.length > 0 && (
          <div className="p-4 rounded-[8px] bg-[#050505] border border-[#6B8C42]/40 space-y-2">
            <div className="flex items-center justify-between">
              <span className="text-xs font-bold text-[#E8D9A8] flex items-center gap-1.5">
                <CheckCircle2 className="w-4 h-4 text-[#8AB55A]" />
                Active Connected Agents ({onboardingState.connectedAgents.length})
              </span>
              <span className="text-[10px] font-mono text-[#8AB55A]">Ready for hardware ops</span>
            </div>

            <div className="flex flex-wrap gap-2">
              {onboardingState.connectedAgents.map((agent, i) => (
                <div
                  key={i}
                  className="px-3 py-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] flex items-center gap-2 text-xs text-[#E8D9A8] font-mono"
                >
                  <span className="w-2 h-2 rounded-full bg-[#8AB55A]"></span>
                  <span>{agent.name}</span>
                  <span className="text-[10px] text-[#8A8577]">({agent.version})</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Action Controls */}
        <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
          <button
            type="button"
            onClick={() => navigateToRoute('onboarding/workspace')}
            className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Back
          </button>

          <div className="flex items-center gap-3">
            <span className="text-[11px] font-mono text-[#8A8577]">
              {onboardingState.connectedAgents.length > 0
                ? `${onboardingState.connectedAgents.length} agent paired`
                : '1 agent required'}
            </span>

            <button
              id="continue-to-device-choice-btn"
              type="button"
              onClick={handleContinue}
              disabled={onboardingState.connectedAgents.length === 0}
              className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors disabled:opacity-40 shadow-sm"
            >
              <span>Continue</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
