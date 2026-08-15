import React, { useState, useEffect, useMemo } from 'react';
import { useLoki } from '../../context/LokiContext';
import { StatusChip } from './StatusChip';
import {
  Search,
  Cpu,
  Bot,
  AlertOctagon,
  Rocket,
  Activity,
  Settings,
  PlusCircle,
  ShieldAlert,
  ArrowRight,
  X,
} from 'lucide-react';

export const CommandPalette: React.FC = () => {
  const {
    isCommandPaletteOpen,
    setIsCommandPaletteOpen,
    devices,
    agentRuns,
    incidents,
    deployments,
    approvals,
    navigateTo,
    setIsDeployModalOpen,
    setIsAddDeviceModalOpen,
    setActiveApprovalModal,
  } = useLoki();

  const [query, setQuery] = useState('');
  const [selectedIndex, setSelectedIndex] = useState(0);

  // Keyboard shortcut listener
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'k') {
        e.preventDefault();
        setIsCommandPaletteOpen(!isCommandPaletteOpen);
      }
      if (e.key === 'Escape' && isCommandPaletteOpen) {
        setIsCommandPaletteOpen(false);
      }
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [isCommandPaletteOpen, setIsCommandPaletteOpen]);

  // Search Results
  const results = useMemo(() => {
    if (!query.trim()) {
      return [
        {
          group: 'Quick Navigation',
          items: [
            { id: 'nav-dash', title: 'Dashboard', subtitle: 'Overview & system health', icon: Activity, action: () => navigateTo('dashboard') },
            { id: 'nav-dev', title: 'Devices & Inventory', subtitle: `${devices.length} hardware nodes`, icon: Cpu, action: () => navigateTo('devices') },
            { id: 'nav-ag', title: 'Agents & Task Runs', subtitle: `${agentRuns.length} active agent sessions`, icon: Bot, action: () => navigateTo('agents') },
            { id: 'nav-inc', title: 'Incidents & Evidence', subtitle: `${incidents.filter(i => i.status !== 'Resolved').length} active incidents`, icon: AlertOctagon, action: () => navigateTo('incidents') },
            { id: 'nav-dep', title: 'Deployments & OTA', subtitle: `${deployments.length} releases logged`, icon: Rocket, action: () => navigateTo('deployments') },
            { id: 'nav-set', title: 'Workspace Settings', subtitle: 'Permissions, bridges, policies', icon: Settings, action: () => navigateTo('settings') },
          ],
        },
        {
          group: 'Quick Actions',
          items: [
            {
              id: 'act-appr',
              title: `Review Approvals (${approvals.filter(a => a.status === 'pending').length} pending)`,
              subtitle: 'Inspect high-risk agent operations',
              icon: ShieldAlert,
              action: () => {
                const pending = approvals.find(a => a.status === 'pending');
                if (pending) setActiveApprovalModal(pending);
                else navigateTo('dashboard');
              },
            },
            {
              id: 'act-new-dep',
              title: 'Create / Deploy Firmware',
              subtitle: 'Rollout artifact to device fleet',
              icon: PlusCircle,
              action: () => setIsDeployModalOpen(true),
            },
            {
              id: 'act-new-dev',
              title: 'Discover / Add Device',
              subtitle: 'Scan local serial and gateway ports',
              icon: Cpu,
              action: () => setIsAddDeviceModalOpen(true),
            },
          ],
        },
      ];
    }

    const q = query.toLowerCase();
    const matchedDevices = devices
      .filter((d) => d.name.toLowerCase().includes(q) || d.hardwareId.toLowerCase().includes(q) || d.family.toLowerCase().includes(q))
      .map((d) => ({
        id: d.id,
        title: d.name,
        subtitle: `${d.family} • ${d.transportPort} • ${d.firmwareVersion}`,
        status: d.status,
        icon: Cpu,
        action: () => navigateTo('devices', d.id),
      }));

    const matchedIncidents = incidents
      .filter((i) => i.title.toLowerCase().includes(q) || i.id.toLowerCase().includes(q) || i.affectedDeviceName.toLowerCase().includes(q))
      .map((i) => ({
        id: i.id,
        title: `${i.id}: ${i.title}`,
        subtitle: `Affected: ${i.affectedDeviceName} • Severity: ${i.severity}`,
        status: i.status === 'Resolved' ? 'Resolved' : 'Running',
        icon: AlertOctagon,
        action: () => navigateTo('incidents', i.id),
      }));

    const matchedAgents = agentRuns
      .filter((a) => a.taskTitle.toLowerCase().includes(q) || a.provider.toLowerCase().includes(q) || a.id.toLowerCase().includes(q))
      .map((a) => ({
        id: a.id,
        title: a.taskTitle,
        subtitle: `${a.provider} • Phase: ${a.phase}`,
        status: a.phase === 'complete' ? 'Completed' : 'Running',
        icon: Bot,
        action: () => navigateTo('agents', a.id),
      }));

    const matchedDeployments = deployments
      .filter((d) => d.releaseTag.toLowerCase().includes(q) || d.id.toLowerCase().includes(q) || d.artifactVersion.toLowerCase().includes(q))
      .map((d) => ({
        id: d.id,
        title: `${d.id}: ${d.releaseTag}`,
        subtitle: `${d.artifactType} • Target: ${d.targetFleetScope}`,
        status: d.phase === 'completed' ? 'Completed' : 'Verifying',
        icon: Rocket,
        action: () => navigateTo('deployments', d.id),
      }));

    const output = [];
    if (matchedDevices.length > 0) output.push({ group: 'Devices', items: matchedDevices });
    if (matchedIncidents.length > 0) output.push({ group: 'Incidents', items: matchedIncidents });
    if (matchedAgents.length > 0) output.push({ group: 'Agent Runs', items: matchedAgents });
    if (matchedDeployments.length > 0) output.push({ group: 'Deployments', items: matchedDeployments });

    return output;
  }, [query, devices, incidents, agentRuns, deployments, approvals, navigateTo, setIsDeployModalOpen, setIsAddDeviceModalOpen, setActiveApprovalModal]);

  if (!isCommandPaletteOpen) return null;

  return (
    <div
      id="command-palette-backdrop"
      className="fixed inset-0 z-50 flex items-start justify-center pt-20 px-4 bg-[#010101]/80 backdrop-blur-sm"
      onClick={() => setIsCommandPaletteOpen(false)}
    >
      <div
        id="command-palette-card"
        className="w-full max-w-xl bg-[#050505] border border-[#23221C] rounded-[14px] shadow-2xl overflow-hidden animate-in fade-in zoom-in-95 duration-100"
        onClick={(e) => e.stopPropagation()}
      >
        {/* Search input */}
        <div className="flex items-center gap-3 px-4 py-3.5 border-b border-[#23221C] bg-[#090909]">
          <Search className="w-4 h-4 text-[#8A8577] shrink-0" />
          <input
            id="command-palette-input"
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Search devices, job IDs, incidents, agents, deployments, or actions..."
            className="w-full bg-transparent text-sm text-[#E8D9A8] placeholder-[#8A8577] focus:outline-none font-sans"
            autoFocus
          />
          <kbd className="hidden sm:inline-flex items-center px-2 py-0.5 rounded text-[10px] font-mono bg-[#0A0A08] border border-[#23221C] text-[#8A8577]">
            ESC
          </kbd>
        </div>

        {/* Results List */}
        <div className="max-h-[60vh] overflow-y-auto p-2 space-y-3">
          {results.length === 0 ? (
            <div className="py-12 text-center text-xs text-[#8A8577]">
              No resources, commands, or job IDs matched <span className="text-[#E8D9A8]">"{query}"</span>
            </div>
          ) : (
            results.map((group, gIdx) => (
              <div key={gIdx} className="space-y-1">
                <div className="px-3 py-1 text-[10px] font-semibold uppercase tracking-wider text-[#8A8577]">
                  {group.group}
                </div>
                {group.items.map((item: any) => {
                  const Icon = item.icon;
                  return (
                    <div
                      key={item.id}
                      id={`cmd-item-${item.id}`}
                      onClick={() => {
                        item.action();
                        setIsCommandPaletteOpen(false);
                      }}
                      className="flex items-center justify-between p-2.5 rounded-[8px] hover:bg-[#0A0A08] hover:border-[#8AB55A]/30 border border-transparent cursor-pointer transition-colors group"
                    >
                      <div className="flex items-center gap-3 min-w-0">
                        <div className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] group-hover:text-[#8AB55A] group-hover:border-[#8AB55A]/40 transition-colors shrink-0">
                          <Icon className="w-4 h-4" />
                        </div>
                        <div className="truncate">
                          <div className="text-xs font-medium text-[#E8D9A8] group-hover:text-[#E8D9A8] truncate">
                            {item.title}
                          </div>
                          <div className="text-[11px] text-[#8A8577] truncate">{item.subtitle}</div>
                        </div>
                      </div>

                      <div className="flex items-center gap-2 shrink-0 ml-2">
                        {item.status && <StatusChip status={item.status} className="text-[10px] py-0" />}
                        <ArrowRight className="w-3.5 h-3.5 text-[#8A8577] opacity-0 group-hover:opacity-100 transition-opacity" />
                      </div>
                    </div>
                  );
                })}
              </div>
            ))
          )}
        </div>

        {/* Footer shortcuts */}
        <div className="flex items-center justify-between px-4 py-2 bg-[#090909] border-t border-[#23221C] text-[11px] text-[#8A8577]">
          <div className="flex items-center gap-3">
            <span>
              <kbd className="font-mono bg-[#0A0A08] px-1.5 py-0.5 rounded border border-[#23221C]">↑↓</kbd> navigate
            </span>
            <span>
              <kbd className="font-mono bg-[#0A0A08] px-1.5 py-0.5 rounded border border-[#23221C]">↵</kbd> select
            </span>
          </div>
          <span className="font-mono text-[10px]">LokiAI Main Command Bus</span>
        </div>
      </div>
    </div>
  );
};
