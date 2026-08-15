import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { ConnectionIndicator } from '../common/ConnectionIndicator';
import { Search, ShieldAlert, Cpu, Rocket } from 'lucide-react';

export const TopBar: React.FC = () => {
  const {
    currentRoute,
    currentWorkspace,
    approvals,
    setActiveApprovalModal,
    setIsCommandPaletteOpen,
    setIsDeployModalOpen,
    setIsAddDeviceModalOpen,
  } = useLoki();

  const pendingApprovals = approvals.filter((a) => a.status === 'pending');

  const getPageTitle = () => {
    switch (currentRoute) {
      case 'dashboard':
        return 'System Health & Action Center';
      case 'devices':
        return 'Hardware Devices & Topologies';
      case 'agents':
        return 'Connected Coding Agents & Task Trees';
      case 'monitoring':
        return 'Watchers, Telemetry & Rule Engine';
      case 'incidents':
        return 'Incidents, Diagnostics & Evidence';
      case 'deployments':
        return 'Firmware Releases & OTA Rollouts';
      case 'activity':
        return 'Immutable Operational Audit Trail';
      case 'settings':
        return 'Workspace & System Configuration';
      default:
        return 'Hardware Operations';
    }
  };

  return (
    <header
      id="main-top-bar"
      className="h-14 lg:h-16 bg-[#050505] border-b border-[#23221C] px-4 lg:px-6 flex items-center justify-between gap-4 z-20 shrink-0 select-none"
    >
      {/* Left: Breadcrumbs & Page Title */}
      <div className="flex items-center gap-3 min-w-0">
        <div className="flex flex-col">
          <div className="flex items-center gap-2 text-[11px] font-mono text-[#8A8577]">
            <span className="text-[#8AB55A] font-semibold uppercase">{currentWorkspace.name}</span>
            <span>/</span>
            <span className="capitalize">{currentRoute}</span>
          </div>
          <h1 className="text-sm lg:text-base font-bold text-[#E8D9A8] truncate font-sans">
            {getPageTitle()}
          </h1>
        </div>
      </div>

      {/* Right: Quick Search, Status, Approvals, Action CTAs */}
      <div className="flex items-center gap-2.5 shrink-0">
        {/* Global Command Bar Trigger */}
        <button
          id="global-search-trigger"
          onClick={() => setIsCommandPaletteOpen(true)}
          className="flex items-center gap-2 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 text-xs text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
        >
          <Search className="w-3.5 h-3.5" />
          <span className="hidden sm:inline">Search or command...</span>
          <kbd className="hidden sm:inline-flex items-center px-1.5 py-0.5 rounded text-[10px] font-mono bg-[#050505] border border-[#23221C] text-[#8A8577]">
            ⌘K
          </kbd>
        </button>

        {/* Runtime / Gateway Connection Indicator */}
        <div className="hidden md:block">
          <ConnectionIndicator />
        </div>

        {/* Approvals Inbox Trigger */}
        <button
          id="approvals-inbox-button"
          onClick={() => {
            if (pendingApprovals.length > 0) {
              setActiveApprovalModal(pendingApprovals[0]);
            }
          }}
          className={`relative flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] border text-xs font-semibold transition-all ${
            pendingApprovals.length > 0
              ? 'bg-[#23221C] border-[#8AB55A] text-[#E8D9A8] hover:bg-[#38362D]'
              : 'bg-[#0A0A08] border-[#23221C] text-[#8A8577] cursor-default'
          }`}
          title="Review Pending Security Approvals"
        >
          <ShieldAlert
            className={`w-3.5 h-3.5 ${pendingApprovals.length > 0 ? 'text-[#8AB55A]' : 'text-[#8A8577]'}`}
          />
          <span className="hidden sm:inline">Approvals</span>
          {pendingApprovals.length > 0 && (
            <span className="px-1.5 py-0.2 rounded-full bg-[#8AB55A] text-[#010101] font-mono text-[10px] font-bold">
              {pendingApprovals.length}
            </span>
          )}
        </button>

        {/* Deploy & Discover Quick Buttons */}
        <button
          onClick={() => setIsDeployModalOpen(true)}
          className="hidden lg:flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] transition-colors text-xs font-bold shadow-sm"
        >
          <Rocket className="w-3.5 h-3.5" />
          <span>Deploy Firmware</span>
        </button>

        <button
          onClick={() => setIsAddDeviceModalOpen(true)}
          className="hidden xl:flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 text-xs font-medium text-[#E8D9A8] transition-colors"
        >
          <Cpu className="w-3.5 h-3.5 text-[#8AB55A]" />
          <span>Add Device</span>
        </button>
      </div>
    </header>
  );
};
