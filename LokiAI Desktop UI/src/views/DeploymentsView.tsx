import React, { useState } from 'react';
import { useLoki } from '../context/LokiContext';
import { Deployment } from '../types';
import { StatusChip } from '../components/common/StatusChip';
import { LogViewer } from '../components/common/LogViewer';
import {
  Rocket,
  PlusCircle,
  ArrowLeft,
  ShieldCheck,
  CheckCircle2,
  AlertTriangle,
  RotateCcw,
  Clock,
  HardDrive,
  FileCode,
  Layers,
  Terminal,
} from 'lucide-react';

export const DeploymentsView: React.FC = () => {
  const {
    deployments,
    selectedDeploymentId,
    setSelectedDeploymentId,
    setIsDeployModalOpen,
    openInspector,
    logs,
    triggerRollback,
  } = useLoki();

  const selectedDeployment = deployments.find((d) => d.id === selectedDeploymentId);

  const phases: Deployment['phase'][] = [
    'queued',
    'preflight',
    'transferring',
    'flashing',
    'rebooting',
    'testing',
    'observing',
    'completed',
  ];

  if (selectedDeployment) {
    const deploymentLogs = logs.filter(
      (l) => l.correlationJobId === selectedDeployment.id || l.service === 'deployment_engine'
    );

    const currentPhaseIndex = phases.indexOf(selectedDeployment.phase);
    const avgProgress = Math.round(
      selectedDeployment.deviceResults.reduce((acc, d) => acc + d.progressPct, 0) /
        (selectedDeployment.deviceResults.length || 1)
    );

    return (
      <div id="deployment-detail-view" className="space-y-6">
        {/* Detail Header */}
        <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setSelectedDeploymentId(null)}
              className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Back to releases"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
            <div>
              <div className="flex items-center gap-2">
                <span className="font-mono font-bold text-sm text-[#8AB55A]">{selectedDeployment.id}</span>
                <h2 className="text-base font-bold text-[#E8D9A8]">{selectedDeployment.releaseTag}</h2>
                <StatusChip
                  status={selectedDeployment.phase === 'completed' || selectedDeployment.phase === 'verified' ? 'Completed' : 'Running'}
                />
              </div>
              <p className="text-xs text-[#8A8577] font-mono mt-0.5">
                Artifact: {selectedDeployment.artifactVersion} ({selectedDeployment.artifactType}) • Scope: {selectedDeployment.targetFleetScope}
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            <button
              onClick={() => openInspector('deployment', selectedDeployment)}
              className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#8AB55A] hover:bg-[#23221C] transition-colors"
            >
              Inspect Signature & Digest
            </button>
          </div>
        </div>

        {/* 9-Phase Progress Bar */}
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 font-mono text-xs">
          <div className="flex items-center justify-between pb-2 border-b border-[#23221C] font-sans">
            <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
              Deployment Lifecycle Execution Phase
            </span>
            <span className="text-[11px] font-mono text-[#8AB55A] uppercase">
              Current: {selectedDeployment.phase} ({avgProgress}%)
            </span>
          </div>

          {/* Stepper Flow */}
          <div className="grid grid-cols-2 sm:grid-cols-4 lg:grid-cols-8 gap-2 pt-2">
            {phases.map((p, idx) => {
              const isPast = idx < currentPhaseIndex;
              const isCurrent = idx === currentPhaseIndex;

              return (
                <div
                  key={p}
                  className={`p-2.5 rounded-[8px] border text-center transition-all ${
                    isCurrent
                      ? 'bg-[#23221C] border-[#8AB55A] text-[#8AB55A] ring-1 ring-[#8AB55A]/50 font-bold'
                      : isPast
                      ? 'bg-[#0A0A08] border-[#23221C] text-[#E8D9A8]'
                      : 'bg-[#050505] border-[#23221C] text-[#8A8577]'
                  }`}
                >
                  <div className="text-[10px] text-[#8A8577] mb-1">Step {idx + 1}</div>
                  <div className="capitalize truncate text-xs">{p}</div>
                </div>
              );
            })}
          </div>
        </div>

        {/* Per-Device Execution Table */}
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 font-mono text-xs">
          <div className="flex items-center justify-between pb-2 border-b border-[#23221C] font-sans">
            <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
              Target Hardware Node Rollout Status
            </span>
            <span className="text-[11px] text-[#8AB55A]">
              Strategy: {selectedDeployment.rolloutStrategy}
            </span>
          </div>

          <div className="space-y-2">
            {selectedDeployment.deviceResults.map((dev) => (
              <div
                key={dev.deviceId}
                className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex flex-wrap items-center justify-between gap-3"
              >
                <div>
                  <div className="font-bold text-[#E8D9A8]">{dev.deviceName}</div>
                  <div className="text-[10px] text-[#8A8577]">{dev.deviceId} • Stage: {dev.currentStage}</div>
                </div>

                <div className="flex items-center gap-4">
                  <div className="w-32 bg-[#050505] rounded-full h-2 border border-[#23221C] overflow-hidden">
                    <div
                      className="bg-[#8AB55A] h-full transition-all duration-300"
                      style={{ width: `${dev.progressPct}%` }}
                    ></div>
                  </div>
                  <span className="w-12 text-right font-bold text-[#E8D9A8]">{dev.progressPct}%</span>
                  <StatusChip status={dev.status === 'Completed' ? 'Completed' : 'Running'} />
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Deployment Logs */}
        <LogViewer logs={deploymentLogs} title={`Deployment Engine Logs (${selectedDeployment.id})`} />
      </div>
    );
  }

  // Deployments List View
  return (
    <div id="deployments-view" className="space-y-6">
      {/* Top Banner */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex items-center gap-3">
          <Rocket className="w-4 h-4 text-[#8AB55A]" />
          <div>
            <h2 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
              Firmware Deployments & Release Management ({deployments.length})
            </h2>
            <p className="text-xs text-[#8A8577] mt-0.5 font-mono">
              Signed artifacts with continuous preflight assertions and rollback verification.
            </p>
          </div>
        </div>

        <button
          onClick={() => setIsDeployModalOpen(true)}
          className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors shadow-sm"
        >
          <PlusCircle className="w-3.5 h-3.5" /> Deploy Firmware Release
        </button>
      </div>

      {/* Deployments Table */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="bg-[#090909] border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-3 px-4">Release Tag & ID</th>
                <th className="py-3 px-4">Artifact Type & Version</th>
                <th className="py-3 px-4">Target Scope</th>
                <th className="py-3 px-4">Current Phase</th>
                <th className="py-3 px-4">Initiator</th>
                <th className="py-3 px-4">Started</th>
                <th className="py-3 px-4 text-right">Inspect</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {deployments.map((dep) => (
                <tr
                  key={dep.id}
                  id={`deployment-row-${dep.id}`}
                  onClick={() => setSelectedDeploymentId(dep.id)}
                  className="hover:bg-[#0A0A08] cursor-pointer transition-colors group"
                >
                  <td className="py-3 px-4">
                    <div className="font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                      {dep.releaseTag}
                    </div>
                    <div className="text-[10px] text-[#8A8577]">{dep.id}</div>
                  </td>
                  <td className="py-3 px-4 font-semibold text-[#8AB55A]">
                    {dep.artifactVersion} <span className="text-[#8A8577] font-normal">({dep.artifactType})</span>
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">{dep.targetFleetScope}</td>
                  <td className="py-3 px-4">
                    <span className="capitalize px-2 py-0.5 rounded bg-[#23221C] text-[#E8D9A8] text-[11px]">
                      {dep.phase}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">{dep.initiator.name}</td>
                  <td className="py-3 px-4 text-[#8A8577]">{dep.startTime}</td>
                  <td className="py-3 px-4 text-right">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setSelectedDeploymentId(dep.id);
                      }}
                      className="px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] text-[#8AB55A] hover:bg-[#23221C] transition-colors"
                    >
                      Track Rollout
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
