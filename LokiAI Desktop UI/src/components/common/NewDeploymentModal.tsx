import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { ArtifactType, RolloutStrategy } from '../../types';
import { X, Rocket, ShieldCheck, AlertTriangle, ArrowRight, ArrowLeft } from 'lucide-react';

export const NewDeploymentModal: React.FC = () => {
  const { isDeployModalOpen, setIsDeployModalOpen, devices, createDeployment } = useLoki();
  const [step, setStep] = useState(1);

  // Form State
  const [releaseTag, setReleaseTag] = useState('v1.4.3-telemetry-hotfix');
  const [artifactType, setArtifactType] = useState<ArtifactType>('Firmware (.bin/.elf)');
  const [artifactVersion, setArtifactVersion] = useState('v1.4.3-patch');
  const [selectedDeviceIds, setSelectedDeviceIds] = useState<string[]>([
    devices[0]?.id || 'dev-esp32-01',
  ]);
  const [rolloutStrategy, setRolloutStrategy] = useState<RolloutStrategy>('Canary (10% Fleet)');
  const [observationWindowSec, setObservationWindowSec] = useState(60);
  const [requirePreflight, setRequirePreflight] = useState(true);

  if (!isDeployModalOpen) return null;

  const toggleDeviceSelection = (id: string) => {
    setSelectedDeviceIds((prev) =>
      prev.includes(id) ? prev.filter((d) => d !== id) : [...prev, id],
    );
  };

  const handleSubmit = () => {
    createDeployment({
      releaseTag,
      artifactType,
      artifactVersion,
      targetFleetScope: `${selectedDeviceIds.length} Target Node(s)`,
      targetDeviceIds: selectedDeviceIds,
      rolloutStrategy,
      observationWindowSec,
    });
  };

  return (
    <div
      id="deploy-modal-backdrop"
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#010101]/80 backdrop-blur-sm p-4 overflow-y-auto"
      role="dialog"
      aria-modal="true"
    >
      <div
        id="deploy-modal-card"
        className="w-full max-w-2xl bg-[#050505] border border-[#23221C] rounded-[14px] shadow-2xl overflow-hidden my-8"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-b border-[#23221C]">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-[8px] bg-[#23221C] text-[#8AB55A]">
              <Rocket className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-[#E8D9A8]">
                Create & Deploy Hardware Release
              </h2>
              <p className="text-xs text-[#8A8577] font-mono">
                Step {step} of 4:{' '}
                {step === 1
                  ? 'Select Artifact'
                  : step === 2
                    ? 'Target Selection'
                    : step === 3
                      ? 'Rollout & Verification'
                      : 'Review & Authorize'}
              </p>
            </div>
          </div>
          <button
            onClick={() => setIsDeployModalOpen(false)}
            className="p-1.5 rounded-[8px] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Wizard Step Content */}
        <div className="p-6 space-y-5 text-xs text-[#E8D9A8]">
          {/* Step 1: Select Artifact */}
          {step === 1 && (
            <div className="space-y-4">
              <div>
                <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                  Release Tag Name
                </label>
                <input
                  type="text"
                  value={releaseTag}
                  onChange={(e) => setReleaseTag(e.target.value)}
                  className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] font-mono text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                />
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div>
                  <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                    Artifact Type
                  </label>
                  <select
                    value={artifactType}
                    onChange={(e) => setArtifactType(e.target.value as ArtifactType)}
                    className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] font-mono text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                  >
                    <option value="Firmware (.bin/.elf)">Firmware (.bin/.elf)</option>
                    <option value="Device Config (.json)">Device Config (.json)</option>
                    <option value="Edge AI Model (.tflite)">Edge AI Model (.tflite)</option>
                    <option value="App Container">App Container</option>
                  </select>
                </div>

                <div>
                  <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                    Artifact Version
                  </label>
                  <input
                    type="text"
                    value={artifactVersion}
                    onChange={(e) => setArtifactVersion(e.target.value)}
                    className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] font-mono text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                  />
                </div>
              </div>

              <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <ShieldCheck className="w-4 h-4 text-[#8AB55A]" />
                  <span className="font-mono text-xs text-[#E8D9A8]">
                    Loki-Ed25519 Cryptographic Build Signature
                  </span>
                </div>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#6B8C42]/20 text-[#8AB55A]">
                  Verified Digest
                </span>
              </div>
            </div>
          )}

          {/* Step 2: Target Selection */}
          {step === 2 && (
            <div className="space-y-4">
              <div className="flex items-center justify-between">
                <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider">
                  Select Compatible Target Hardware Nodes ({selectedDeviceIds.length} selected)
                </span>
                <button
                  onClick={() => setSelectedDeviceIds(devices.map((d) => d.id))}
                  className="text-[11px] text-[#8AB55A] hover:underline font-mono"
                >
                  Select All Fleet
                </button>
              </div>

              <div className="max-h-60 overflow-y-auto space-y-2">
                {devices.map((dev) => {
                  const isSelected = selectedDeviceIds.includes(dev.id);
                  return (
                    <div
                      key={dev.id}
                      onClick={() => toggleDeviceSelection(dev.id)}
                      className={`cursor-pointer p-3 rounded-[8px] border transition-all flex items-center justify-between ${
                        isSelected
                          ? 'bg-[#0A0A08] border-[#8AB55A] ring-1 ring-[#8AB55A]/40'
                          : 'bg-[#050505] border-[#23221C] hover:border-[#8A8577]'
                      }`}
                    >
                      <div className="flex items-center gap-3">
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={() => {}}
                          className="accent-[#8AB55A] rounded"
                        />
                        <div>
                          <div className="font-mono font-semibold text-xs text-[#E8D9A8]">
                            {dev.name}
                          </div>
                          <div className="text-[11px] text-[#8A8577] font-mono">
                            {dev.family} • {dev.transportPort} • Current: {dev.firmwareVersion}
                          </div>
                        </div>
                      </div>

                      <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8A8577]">
                        {dev.supportTier}
                      </span>
                    </div>
                  );
                })}
              </div>
            </div>
          )}

          {/* Step 3: Rollout Strategy & Verification */}
          {step === 3 && (
            <div className="space-y-4">
              <div>
                <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                  Rollout Strategy
                </label>
                <div className="grid grid-cols-3 gap-2">
                  {(
                    [
                      'All-at-Once',
                      'Canary (10% Fleet)',
                      'Rolling Batches (2x2)',
                    ] as RolloutStrategy[]
                  ).map((strat) => (
                    <button
                      key={strat}
                      type="button"
                      onClick={() => setRolloutStrategy(strat)}
                      className={`p-2.5 rounded-[8px] border text-left text-xs transition-all font-mono ${
                        rolloutStrategy === strat
                          ? 'bg-[#23221C] border-[#8AB55A] text-[#E8D9A8]'
                          : 'bg-[#0A0A08] border-[#23221C] text-[#8A8577] hover:border-[#8A8577]'
                      }`}
                    >
                      {strat}
                    </button>
                  ))}
                </div>
              </div>

              <div>
                <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                  Post-Deployment Stability Observation Window
                </label>
                <div className="grid grid-cols-3 gap-2 font-mono">
                  {[30, 60, 180].map((sec) => (
                    <button
                      key={sec}
                      type="button"
                      onClick={() => setObservationWindowSec(sec)}
                      className={`p-2.5 rounded-[8px] border text-center text-xs transition-all ${
                        observationWindowSec === sec
                          ? 'bg-[#23221C] border-[#8AB55A] text-[#8AB55A] font-semibold'
                          : 'bg-[#0A0A08] border-[#23221C] text-[#8A8577] hover:border-[#8A8577]'
                      }`}
                    >
                      {sec} Seconds
                    </button>
                  ))}
                </div>
              </div>

              <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between">
                <div>
                  <span className="text-xs font-semibold text-[#E8D9A8] block">
                    Automated Hardware Preflight Checks
                  </span>
                  <span className="text-[11px] text-[#8A8577]">
                    Assert header integrity, memory capacity, and transport ping before flashing.
                  </span>
                </div>
                <input
                  type="checkbox"
                  checked={requirePreflight}
                  onChange={(e) => setRequirePreflight(e.target.checked)}
                  className="accent-[#8AB55A] h-4 w-4 rounded"
                />
              </div>
            </div>
          )}

          {/* Step 4: Review & Authorize */}
          {step === 4 && (
            <div className="space-y-4 font-mono">
              <div className="p-4 rounded-[10px] bg-[#0A0A08] border border-[#23221C] space-y-2">
                <div className="flex justify-between border-b border-[#23221C] pb-2 font-sans">
                  <span className="text-[#8A8577] font-semibold uppercase">Deployment Summary</span>
                  <span className="text-[#8AB55A] font-bold">{releaseTag}</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-[#8A8577]">Artifact:</span>{' '}
                  <span>
                    {artifactVersion} ({artifactType})
                  </span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-[#8A8577]">Target Nodes:</span>{' '}
                  <span>{selectedDeviceIds.length} hardware unit(s)</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-[#8A8577]">Rollout Strategy:</span>{' '}
                  <span>{rolloutStrategy}</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-[#8A8577]">Observation Window:</span>{' '}
                  <span>{observationWindowSec}s mandatory</span>
                </div>
                <div className="flex justify-between text-xs">
                  <span className="text-[#8A8577]">Rollback Safeguard:</span>{' '}
                  <span className="text-[#8AB55A]">Enabled (Cached verified fallback)</span>
                </div>
              </div>

              <div className="p-3 rounded-[8px] bg-[#23221C]/40 border border-[#23221C] flex gap-2.5 items-start font-sans text-xs">
                <AlertTriangle className="w-4 h-4 text-[#E8D9A8] shrink-0 mt-0.5" />
                <span className="text-[#8A8577]">
                  Deployment will acquire serial locks on target hardware nodes. Never mark
                  deployment successful until post-flash observation passes.
                </span>
              </div>
            </div>
          )}
        </div>

        {/* Footer Navigation */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-t border-[#23221C]">
          {step > 1 ? (
            <button
              onClick={() => setStep((s) => s - 1)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-medium text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            >
              <ArrowLeft className="w-3.5 h-3.5" /> Back
            </button>
          ) : (
            <button
              onClick={() => setIsDeployModalOpen(false)}
              className="px-3 py-1.5 rounded-[8px] bg-transparent border border-[#23221C] text-xs font-medium text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            >
              Cancel
            </button>
          )}

          {step < 4 ? (
            <button
              onClick={() => setStep((s) => s + 1)}
              className="flex items-center gap-1.5 px-4 py-1.5 rounded-[8px] bg-[#8AB55A] text-xs font-semibold text-[#010101] hover:bg-[#A8C878] transition-colors"
            >
              Next Step <ArrowRight className="w-3.5 h-3.5" />
            </button>
          ) : (
            <button
              onClick={handleSubmit}
              className="flex items-center gap-1.5 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors shadow-sm"
            >
              <Rocket className="w-3.5 h-3.5" /> Confirm & Start Deployment
            </button>
          )}
        </div>
      </div>
    </div>
  );
};
