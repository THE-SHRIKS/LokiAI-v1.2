import React, { useState, useEffect } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import {
  ShieldCheck,
  RefreshCw,
  CheckCircle2,
  AlertTriangle,
  XCircle,
  ArrowRight,
  ArrowLeft,
} from 'lucide-react';

interface CheckPhase {
  id: string;
  name: string;
  description: string;
  details: string;
  status: 'pending' | 'running' | 'passed' | 'warning' | 'failed';
}

export const DeviceHealthCheckView: React.FC = () => {
  const { onboardingState, updateOnboardingState, navigateToRoute, addDiscoveredDevice } =
    useLoki();

  const targetDev = onboardingState.discoveredDevice || {
    id: 'disc-01',
    name: 'Chamber-Node-Alpha',
    family: 'ESP32-S3',
    connector: 'USB-CDC Serial',
    transportPort: '/dev/ttyUSB0',
    supportTier: 'Tier-1 Certified',
    healthStatus: 'Healthy',
  };

  const initialPhases: CheckPhase[] = [
    {
      id: '1',
      name: 'Identity Verification',
      description: 'Silicon chip signature & JTAG IDCODE validation',
      details: 'ESP32-S3 (WROOM-1 dual core LX7)',
      status: 'pending',
    },
    {
      id: '2',
      name: 'Connection Validation',
      description: 'Transport ping, round-trip latency & packet loss check',
      details: 'Latency: 1.2ms • Drop rate: 0.0%',
      status: 'pending',
    },
    {
      id: '3',
      name: 'Capability Discovery',
      description: 'Querying hardware control primitives & registers',
      details: 'flash, restart, telemetry, rollback active',
      status: 'pending',
    },
    {
      id: '4',
      name: 'Firmware Identification',
      description: 'Bootloader hash & active firmware slot check',
      details: 'v2.1.0-factory-baseline (Valid SHA256)',
      status: 'pending',
    },
    {
      id: '5',
      name: 'Heartbeat & Telemetry Response',
      description: 'Continuous 10Hz health stream watchdog verification',
      details: 'Watchdog margin: 388ms (Nominal)',
      status: 'pending',
    },
    {
      id: '6',
      name: 'Available LokiAI Operations',
      description: 'Enforce agent sandbox boundaries and safety gates',
      details: '7/8 Operations permitted with approval gates',
      status: 'pending',
    },
    {
      id: '7',
      name: 'Deployment & Rollback Support',
      description: 'Dual-partition A/B OTA flash storage test',
      details: 'OTA slot_0 active, slot_1 rollback ready',
      status: 'pending',
    },
  ];

  const [phases, setPhases] = useState<CheckPhase[]>(initialPhases);
  const [isRunning, setIsRunning] = useState(true);
  const [customDeviceName, setCustomDeviceName] = useState(targetDev.name);
  const [overallOutcome, setOverallOutcome] = useState<
    'ready' | 'limited' | 'unhealthy' | 'running'
  >('running');

  // Automated sequence runner
  useEffect(() => {
    let currentIdx = 0;
    setIsRunning(true);
    setOverallOutcome('running');

    const interval = setInterval(() => {
      if (currentIdx < initialPhases.length) {
        const stepIdx = currentIdx;
        setPhases((prev) =>
          prev.map((p, idx) => {
            if (idx === stepIdx) return { ...p, status: 'passed' };
            if (idx === stepIdx + 1) return { ...p, status: 'running' };
            return p;
          }),
        );
        currentIdx++;
      } else {
        clearInterval(interval);
        setIsRunning(false);
        setOverallOutcome('ready');
      }
    }, 450);

    return () => clearInterval(interval);
  }, []);

  const handleRetry = (scenario?: 'ready' | 'limited' | 'unhealthy') => {
    setIsRunning(true);
    setOverallOutcome('running');
    setPhases(initialPhases);

    setTimeout(() => {
      if (scenario === 'limited') {
        setPhases((prev) =>
          prev.map((p, idx) => {
            if (idx === 6)
              return {
                ...p,
                status: 'warning',
                details: 'Single partition only (No A/B rollback)',
              };
            return { ...p, status: 'passed' };
          }),
        );
        setOverallOutcome('limited');
      } else if (scenario === 'unhealthy') {
        setPhases((prev) =>
          prev.map((p, idx) => {
            if (idx === 4)
              return {
                ...p,
                status: 'failed',
                details: 'Watchdog timeout > 1200ms • Packet loss 14%',
              };
            if (idx > 4) return { ...p, status: 'pending' };
            return { ...p, status: 'passed' };
          }),
        );
        setOverallOutcome('unhealthy');
      } else {
        setPhases((prev) => prev.map((p) => ({ ...p, status: 'passed' })));
        setOverallOutcome('ready');
      }
      setIsRunning(false);
    }, 1500);
  };

  const handleContinue = () => {
    // Add device to context
    addDiscoveredDevice({
      name: customDeviceName.trim() || targetDev.name,
      family: targetDev.family,
      connector: targetDev.connector,
      transportPort: targetDev.transportPort,
    });

    updateOnboardingState({
      discoveredDevice: {
        ...targetDev,
        name: customDeviceName.trim() || targetDev.name,
        healthStatus: overallOutcome === 'ready' ? 'Healthy' : 'Degraded',
        healthChecks: phases,
      },
      currentStep: 'ready',
    });

    navigateToRoute('onboarding/ready');
  };

  return (
    <div
      id="device-health-screen"
      className="w-full max-w-4xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200"
    >
      <OnboardingStepper currentStep="device" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
              <ShieldCheck className="w-3.5 h-3.5" /> Step 6: Initial Device Health Validation
            </div>
            <h1 className="text-xl font-bold text-[#E8D9A8]">Checking device health</h1>
            <p className="text-xs text-[#8A8577] mt-1">
              Target: <strong className="text-[#E8D9A8] font-mono">{targetDev.name}</strong> (
              {targetDev.family}) via {targetDev.transportPort}
            </p>
          </div>

          {/* Status Badge */}
          <div>
            {isRunning ? (
              <span className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-[#050505] border border-[#8AB55A]/40 text-[#8AB55A] font-mono text-xs">
                <RefreshCw className="w-3.5 h-3.5 animate-spin" /> Running 7 Phases...
              </span>
            ) : overallOutcome === 'ready' ? (
              <span className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-[#6B8C42]/20 border border-[#8AB55A] text-[#8AB55A] font-mono text-xs font-bold">
                <CheckCircle2 className="w-3.5 h-3.5" /> 7/7 Checks Passed (Ready)
              </span>
            ) : overallOutcome === 'limited' ? (
              <span className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-[#E8D9A8]/10 border border-[#E8D9A8] text-[#E8D9A8] font-mono text-xs">
                <AlertTriangle className="w-3.5 h-3.5" /> Ready with Limitations
              </span>
            ) : (
              <span className="flex items-center gap-1.5 px-3 py-1 rounded-full bg-[#CC272E]/20 border border-[#CC272E] text-[#CC272E] font-mono text-xs font-bold">
                <XCircle className="w-3.5 h-3.5" /> Check Failed
              </span>
            )}
          </div>
        </div>

        {/* 7 Check Phases List */}
        <div className="space-y-2">
          {phases.map((phase, idx) => {
            return (
              <div
                key={phase.id}
                className={`p-3.5 rounded-[8px] border flex items-center justify-between gap-3 text-xs transition-all ${
                  phase.status === 'passed'
                    ? 'bg-[#050505] border-[#23221C]'
                    : phase.status === 'running'
                      ? 'bg-[#6B8C42]/10 border-[#8AB55A]'
                      : phase.status === 'warning'
                        ? 'bg-[#E8D9A8]/5 border-[#E8D9A8]/40'
                        : phase.status === 'failed'
                          ? 'bg-[#CC272E]/10 border-[#CC272E]/40'
                          : 'bg-[#050505] border-[#23221C]/50 opacity-60'
                }`}
              >
                <div className="flex items-center gap-3 min-w-0">
                  <div className="w-6 h-6 rounded-full flex items-center justify-center shrink-0 font-mono text-[10px]">
                    {phase.status === 'passed' && (
                      <CheckCircle2 className="w-4 h-4 text-[#8AB55A]" />
                    )}
                    {phase.status === 'running' && (
                      <RefreshCw className="w-4 h-4 animate-spin text-[#8AB55A]" />
                    )}
                    {phase.status === 'warning' && (
                      <AlertTriangle className="w-4 h-4 text-[#E8D9A8]" />
                    )}
                    {phase.status === 'failed' && <XCircle className="w-4 h-4 text-[#CC272E]" />}
                    {phase.status === 'pending' && (
                      <span className="text-[#8A8577]">{idx + 1}</span>
                    )}
                  </div>

                  <div className="min-w-0">
                    <div className="font-semibold text-[#E8D9A8] flex items-center gap-2">
                      <span>{phase.name}</span>
                      {phase.status === 'passed' && (
                        <span className="text-[10px] text-[#8AB55A] font-mono font-normal">
                          Passed
                        </span>
                      )}
                    </div>
                    <div className="text-[11px] text-[#8A8577] truncate">{phase.description}</div>
                  </div>
                </div>

                <div className="text-[11px] font-mono text-right shrink-0">
                  <span
                    className={
                      phase.status === 'failed'
                        ? 'text-[#CC272E]'
                        : phase.status === 'warning'
                          ? 'text-[#E8D9A8]'
                          : 'text-[#8A8577]'
                    }
                  >
                    {phase.details}
                  </span>
                </div>
              </div>
            );
          })}
        </div>

        {/* Device Naming & Association Form after check completes */}
        {!isRunning && overallOutcome !== 'unhealthy' && (
          <div className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-3 animate-in fade-in">
            <div className="text-xs font-semibold text-[#E8D9A8]">
              Workspace Hardware Association
            </div>
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-3 text-xs">
              <div>
                <label className="block text-[11px] text-[#8A8577] mb-1">
                  Friendly Device Identifier
                </label>
                <input
                  type="text"
                  value={customDeviceName}
                  onChange={(e) => setCustomDeviceName(e.target.value)}
                  className="w-full px-3 py-2 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                />
              </div>

              <div>
                <label className="block text-[11px] text-[#8A8577] mb-1">Assigned Workspace</label>
                <div className="px-3 py-2 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#8AB55A]">
                  {onboardingState.workspace.name} ({onboardingState.workspace.environment})
                </div>
              </div>
            </div>
          </div>
        )}

        {/* Prototype Edge Simulation Buttons */}
        <div className="p-3 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-1.5 text-xs">
          <span className="text-[10px] font-mono uppercase text-[#8A8577]">
            Simulate Health Scenarios:
          </span>
          <div className="flex flex-wrap gap-2 text-[11px] font-mono">
            <button
              onClick={() => handleRetry('ready')}
              className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8AB55A]"
            >
              All 7 Passed (Ready)
            </button>
            <button
              onClick={() => handleRetry('limited')}
              className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#E8D9A8]"
            >
              Passed with Limitations (No A/B Flash)
            </button>
            <button
              onClick={() => handleRetry('unhealthy')}
              className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#CC272E]"
            >
              Watchdog Failure (Unhealthy)
            </button>
          </div>
        </div>

        {/* Footer Navigation */}
        <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
          <button
            type="button"
            onClick={() => navigateToRoute('onboarding/device-connect')}
            className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Choose another device
          </button>

          <div className="flex items-center gap-2">
            {overallOutcome === 'unhealthy' && (
              <button
                type="button"
                onClick={() => handleRetry('ready')}
                className="px-4 py-2 rounded-[8px] bg-[#23221C] text-[#E8D9A8] text-xs font-semibold hover:bg-[#38362D]"
              >
                Retry Health Checks
              </button>
            )}

            <button
              id="continue-to-ready-btn"
              type="button"
              onClick={handleContinue}
              disabled={isRunning || overallOutcome === 'unhealthy'}
              className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors disabled:opacity-40 shadow-sm"
            >
              <span>
                {overallOutcome === 'limited' ? 'Continue with limitations' : 'Continue to Review'}
              </span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
