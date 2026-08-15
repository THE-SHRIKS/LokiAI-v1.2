import React from 'react';
import { useLoki } from '../context/LokiContext';
import { MetricTile } from '../components/common/MetricTile';
import { StatusChip } from '../components/common/StatusChip';
import { SeverityBadge } from '../components/common/SeverityBadge';
import { TelemetryChart } from '../components/common/TelemetryChart';
import { OnboardingChecklistBanner } from '../components/dashboard/OnboardingChecklistBanner';
import {
  Cpu,
  Bot,
  AlertOctagon,
  Rocket,
  ShieldAlert,
  Clock,
  ArrowRight,
  CheckCircle2,
  AlertTriangle,
  Play,
  Terminal,
  Activity,
  PlusCircle,
} from 'lucide-react';

export const DashboardView: React.FC = () => {
  const {
    devices,
    agentRuns,
    incidents,
    deployments,
    approvals,
    navigateTo,
    setActiveApprovalModal,
    setIsDeployModalOpen,
    setIsAddDeviceModalOpen,
    currentWorkspace,
    lastSyncTime,
  } = useLoki();

  const healthyDevices = devices.filter((d) => d.status === 'Healthy').length;
  const degradedDevices = devices.filter((d) => d.status === 'Degraded').length;
  const unhealthyDevices = devices.filter((d) => d.status === 'Unhealthy').length;

  const runningAgents = agentRuns.filter((a) => a.phase !== 'complete' && a.phase !== 'failed').length;
  const activeIncidents = incidents.filter((i) => i.status !== 'Resolved');
  const criticalIncidents = incidents.filter((i) => i.severity === 'Critical' && i.status !== 'Resolved');

  const pendingApprovals = approvals.filter((a) => a.status === 'pending');

  return (
    <div id="dashboard-view" className="space-y-6">
      {/* Workspace Operational Header Banner */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex items-center gap-3">
          <div className="w-2.5 h-2.5 rounded-full bg-[#8AB55A] animate-pulse"></div>
          <div>
            <div className="flex items-center gap-2">
              <h2 className="text-sm font-bold text-[#E8D9A8] font-mono">{currentWorkspace.name}</h2>
              <span className="text-[10px] font-mono uppercase px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A]">
                {currentWorkspace.environment}
              </span>
            </div>
            <p className="text-xs text-[#8A8577] mt-0.5">
              Region: {currentWorkspace.region} • Sync: <span className="text-[#E8D9A8] font-mono">{lastSyncTime}</span>
            </p>
          </div>
        </div>

        <div className="flex items-center gap-2">
          {pendingApprovals.length > 0 ? (
            <button
              onClick={() => setActiveApprovalModal(pendingApprovals[0])}
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] bg-[#23221C] border border-[#8AB55A] text-xs font-semibold text-[#E8D9A8] hover:bg-[#38362D] transition-colors"
            >
              <ShieldAlert className="w-3.5 h-3.5 text-[#8AB55A]" />
              <span>Review {pendingApprovals.length} Approval{pendingApprovals.length > 1 ? 's' : ''}</span>
            </button>
          ) : (
            <button
              onClick={() => setIsDeployModalOpen(true)}
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors"
            >
              <Rocket className="w-3.5 h-3.5" />
              <span>Deploy Release</span>
            </button>
          )}

          <button
            onClick={() => setIsAddDeviceModalOpen(true)}
            className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 text-xs font-medium text-[#E8D9A8] transition-colors"
          >
            <PlusCircle className="w-3.5 h-3.5 text-[#8A8577]" />
            <span>Add Device</span>
          </button>
        </div>
      </div>

      {/* Dismissible Setup Checklist for First-Time Setup */}
      <OnboardingChecklistBanner />

      {/* 1st Row: Compact Operational Summary Tiles */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        <MetricTile
          id="tile-devices"
          label="Hardware Devices"
          value={devices.length}
          subValue={`${healthyDevices} healthy, ${degradedDevices + unhealthyDevices} attention`}
          icon={<Cpu className="w-4 h-4" />}
          statusColor={unhealthyDevices > 0 ? 'destructive' : degradedDevices > 0 ? 'warning' : 'olive'}
          badge="6 Nodes"
          onClick={() => navigateTo('devices')}
        />
        <MetricTile
          id="tile-agents"
          label="Active Agents"
          value={runningAgents}
          subValue={`${agentRuns.length} runs registered`}
          icon={<Bot className="w-4 h-4" />}
          statusColor="olive"
          badge="4 Clients"
          onClick={() => navigateTo('agents')}
        />
        <MetricTile
          id="tile-incidents"
          label="Open Incidents"
          value={activeIncidents.length}
          subValue={criticalIncidents.length > 0 ? `${criticalIncidents.length} Critical` : '0 Critical'}
          icon={<AlertOctagon className="w-4 h-4" />}
          statusColor={criticalIncidents.length > 0 ? 'destructive' : activeIncidents.length > 0 ? 'warning' : 'olive'}
          badge="Newest: 18m"
          onClick={() => navigateTo('incidents')}
        />
        <MetricTile
          id="tile-deployments"
          label="Deployments"
          value={deployments.filter((d) => d.phase === 'observing' || d.phase === 'flashing').length}
          subValue="1 Observing, 2 Verified"
          icon={<Rocket className="w-4 h-4" />}
          statusColor="olive"
          badge="v1.4.2"
          onClick={() => navigateTo('deployments')}
        />
      </div>

      {/* 2nd Row: Action Center ("Needs Your Attention") */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3">
        <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
          <div className="flex items-center gap-2">
            <ShieldAlert className="w-4 h-4 text-[#8AB55A]" />
            <h3 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
              Action Center • Needs Attention ({pendingApprovals.length + activeIncidents.length})
            </h3>
          </div>
          <span className="text-[11px] font-mono text-[#8A8577]">Sorted by severity & age</span>
        </div>

        <div className="space-y-2">
          {/* Pending Approval Requests */}
          {pendingApprovals.map((appr) => (
            <div
              key={appr.id}
              id={`attention-appr-${appr.id}`}
              className="flex flex-wrap items-center justify-between gap-3 p-3 rounded-[8px] bg-[#0A0A08] border border-[#8AB55A]/40 hover:border-[#8AB55A] transition-all group"
            >
              <div className="flex items-start gap-3 min-w-0">
                <div className="p-1.5 rounded-[6px] bg-[#23221C] text-[#8AB55A] shrink-0 mt-0.5">
                  <ShieldAlert className="w-4 h-4" />
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <span className="text-xs font-bold text-[#E8D9A8]">Approval Required: {appr.operationName}</span>
                    <SeverityBadge severity={appr.risk} />
                    <span className="text-[10px] font-mono text-[#8A8577] flex items-center gap-1">
                      <Clock className="w-3 h-3" /> {appr.remainingMinutes}m left
                    </span>
                  </div>
                  <p className="text-xs text-[#8A8577] mt-0.5 line-clamp-1">
                    Requested by <strong className="text-[#E8D9A8]">{appr.requestingActor.name}</strong> for target{' '}
                    <strong className="text-[#E8D9A8] font-mono">{appr.targetDeviceNames.join(', ')}</strong>: {appr.reason}
                  </p>
                </div>
              </div>

              <div className="flex items-center gap-2">
                <button
                  onClick={() => setActiveApprovalModal(appr)}
                  className="px-3 py-1.5 rounded-[6px] bg-[#8AB55A] text-[#010101] text-xs font-semibold hover:bg-[#A8C878] transition-colors"
                >
                  Review & Authorize
                </button>
              </div>
            </div>
          ))}

          {/* Active Incidents */}
          {activeIncidents.map((inc) => (
            <div
              key={inc.id}
              id={`attention-inc-${inc.id}`}
              className="flex flex-wrap items-center justify-between gap-3 p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/30 transition-all group"
            >
              <div className="flex items-start gap-3 min-w-0">
                <div className="p-1.5 rounded-[6px] bg-[#050505] border border-[#23221C] text-[#E8D9A8] shrink-0 mt-0.5">
                  <AlertOctagon className={`w-4 h-4 ${inc.severity === 'Critical' ? 'text-[#CC272E]' : 'text-[#E8D9A8]'}`} />
                </div>
                <div>
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-xs font-bold text-[#E8D9A8]">{inc.id}: {inc.title}</span>
                    <SeverityBadge severity={inc.severity} />
                    <StatusChip status={inc.status === 'Verifying' ? 'Verifying' : 'Running'} />
                  </div>
                  <p className="text-xs text-[#8A8577] mt-0.5">
                    Affected: <span className="text-[#E8D9A8] font-mono">{inc.affectedDeviceName}</span> • Opened: {inc.openedAt} • Assignee: {inc.assignedAgent || 'Unassigned'}
                  </p>
                </div>
              </div>

              <button
                onClick={() => navigateTo('incidents', inc.id)}
                className="flex items-center gap-1 text-xs text-[#8AB55A] hover:underline font-mono"
              >
                Inspect Evidence <ArrowRight className="w-3.5 h-3.5" />
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* 3rd Row: Operational Panels (Telemetry Trend & Active Agents / Deployments) */}
      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Real Hardware Telemetry Chart */}
        <TelemetryChart title="Live Chamber VCC & Thermal Telemetry" />

        {/* Active Coding Agent Work */}
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 flex flex-col justify-between">
          <div>
            <div className="flex items-center justify-between pb-3 border-b border-[#23221C] mb-3">
              <div className="flex items-center gap-2">
                <Bot className="w-4 h-4 text-[#8AB55A]" />
                <h3 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
                  Active Coding Agent Tasks
                </h3>
              </div>
              <button
                onClick={() => navigateTo('agents')}
                className="text-xs text-[#8AB55A] hover:underline font-mono"
              >
                View all agents
              </button>
            </div>

            <div className="space-y-2.5">
              {agentRuns.slice(0, 3).map((run) => (
                <div
                  key={run.id}
                  onClick={() => navigateTo('agents', run.id)}
                  className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/30 transition-all cursor-pointer space-y-1.5"
                >
                  <div className="flex items-center justify-between">
                    <span className="font-semibold text-xs text-[#E8D9A8] line-clamp-1">{run.taskTitle}</span>
                    <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#23221C] text-[#8AB55A]">
                      {run.provider}
                    </span>
                  </div>
                  <p className="text-xs text-[#8A8577] line-clamp-1 font-sans">{run.operationalSummary}</p>
                  <div className="flex items-center justify-between text-[11px] font-mono text-[#8A8577] pt-1">
                    <span className="capitalize text-[#8AB55A]">Phase: {run.phase}</span>
                    <span>Elapsed: {Math.round(run.elapsedSeconds / 60)}m</span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
};
