import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { PrimaryRoute, Workspace } from '../../types';
import {
  LayoutDashboard,
  Cpu,
  Bot,
  Activity,
  AlertOctagon,
  Rocket,
  History,
  Settings,
  ChevronLeft,
  ChevronRight,
  ChevronDown,
  ShieldAlert,
  Radio,
  User,
  ExternalLink,
  LogOut,
  Layers,
} from 'lucide-react';

export const Sidebar: React.FC = () => {
  const {
    currentRoute,
    setCurrentRoute,
    currentWorkspace,
    workspaces,
    setWorkspace,
    incidents,
    approvals,
    deployments,
    userSession,
    setIsLogoutModalOpen,
    setIsAccountModalOpen,
  } = useLoki();

  const [isCollapsed, setIsCollapsed] = useState(false);
  const [isWorkspaceDropdownOpen, setIsWorkspaceDropdownOpen] = useState(false);
  const [isUserMenuOpen, setIsUserMenuOpen] = useState(false);

  const activeIncidentsCount = incidents.filter((i) => i.status !== 'Resolved').length;
  const pendingApprovalsCount = approvals.filter((a) => a.status === 'pending').length;
  const failedDeploymentsCount = deployments.filter((d) => d.phase === 'failed').length;

  const navItems: { id: PrimaryRoute; label: string; icon: any; badgeCount?: number }[] = [
    { id: 'dashboard', label: 'Dashboard', icon: LayoutDashboard },
    { id: 'devices', label: 'Devices', icon: Cpu },
    { id: 'agents', label: 'Agents', icon: Bot },
    { id: 'monitoring', label: 'Monitoring', icon: Activity },
    { id: 'incidents', label: 'Incidents', icon: AlertOctagon, badgeCount: activeIncidentsCount },
    { id: 'deployments', label: 'Deployments', icon: Rocket, badgeCount: failedDeploymentsCount },
    { id: 'activity', label: 'Activity', icon: History },
    { id: 'settings', label: 'Settings', icon: Settings },
  ];

  return (
    <aside
      id="main-app-sidebar"
      className={`relative h-screen bg-[#050505] border-r border-[#23221C] flex flex-col justify-between transition-all duration-200 z-30 shrink-0 select-none ${
        isCollapsed ? 'w-16' : 'w-60 lg:w-64'
      }`}
      aria-label="Application navigation"
    >
      {/* Top Section: Brand & Workspace */}
      <div className="p-3 border-b border-[#23221C]">
        {/* Wordmark Header */}
        <div className="flex items-center justify-between mb-3 px-1">
          <div className="flex items-center gap-2.5 overflow-hidden">
            {/* Minimalist Chamber & Olive Hexagon Logo */}
            <div className="w-7 h-7 rounded-[6px] bg-[#0A0A08] border border-[#6B8C42]/50 flex items-center justify-center text-[#8AB55A] font-bold text-xs shrink-0 shadow-sm">
              λ
            </div>
            {!isCollapsed && (
              <div className="flex flex-col">
                <span className="text-sm font-bold tracking-tight text-[#E8D9A8] font-sans">
                  LOKI<span className="text-[#8AB55A]">AI</span>
                </span>
                <span className="text-[9px] font-mono uppercase tracking-widest text-[#8A8577]">
                  Main • v1.2 Core
                </span>
              </div>
            )}
          </div>

          <button
            onClick={() => setIsCollapsed(!isCollapsed)}
            className="p-1 rounded-[6px] text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08] transition-colors"
            title={isCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
            aria-label="Toggle sidebar collapse"
          >
            {isCollapsed ? <ChevronRight className="w-4 h-4" /> : <ChevronLeft className="w-4 h-4" />}
          </button>
        </div>

        {/* Workspace Selector */}
        {!isCollapsed ? (
          <div className="relative">
            <button
              onClick={() => setIsWorkspaceDropdownOpen(!isWorkspaceDropdownOpen)}
              className="w-full flex items-center justify-between p-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/40 transition-colors text-left"
            >
              <div className="min-w-0 pr-1">
                <div className="text-xs font-semibold text-[#E8D9A8] truncate font-mono">
                  {currentWorkspace.name}
                </div>
                <div className="flex items-center gap-1.5 text-[10px] text-[#8A8577]">
                  <span
                    className={`w-1.5 h-1.5 rounded-full ${
                      currentWorkspace.runtimeConnected ? 'bg-[#8AB55A]' : 'bg-[#CC272E]'
                    }`}
                  ></span>
                  <span className="capitalize">{currentWorkspace.environment}</span>
                </div>
              </div>
              <ChevronDown className="w-3.5 h-3.5 text-[#8A8577] shrink-0" />
            </button>

            {/* Dropdown Menu */}
            {isWorkspaceDropdownOpen && (
              <div className="absolute top-full left-0 right-0 mt-1.5 bg-[#0F0F0C] border border-[#23221C] rounded-[8px] shadow-2xl p-1 z-50 space-y-1">
                <div className="px-2 py-1 text-[10px] uppercase font-semibold text-[#8A8577] tracking-wider">
                  Select Workspace
                </div>
                {workspaces.map((ws) => (
                  <button
                    key={ws.id}
                    onClick={() => {
                      setWorkspace(ws);
                      setIsWorkspaceDropdownOpen(false);
                    }}
                    className={`w-full flex items-center justify-between px-2.5 py-1.5 rounded-[6px] text-xs text-left transition-colors font-mono ${
                      ws.id === currentWorkspace.id
                        ? 'bg-[#23221C] text-[#8AB55A] font-semibold'
                        : 'text-[#E8D9A8] hover:bg-[#0A0A08]'
                    }`}
                  >
                    <span>{ws.name}</span>
                    <span className="text-[10px] text-[#8A8577] uppercase">{ws.environment}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        ) : (
          <div className="flex justify-center py-1">
            <span
              className={`w-2.5 h-2.5 rounded-full ${
                currentWorkspace.runtimeConnected ? 'bg-[#8AB55A]' : 'bg-[#CC272E]'
              }`}
              title={`Workspace: ${currentWorkspace.name}`}
            ></span>
          </div>
        )}
      </div>

      {/* Primary Navigation Routes */}
      <nav className="flex-1 py-3 px-2 space-y-1 overflow-y-auto" aria-label="Primary areas">
        {navItems.map((item) => {
          const Icon = item.icon;
          const isActive = currentRoute === item.id;

          return (
            <button
              key={item.id}
              id={`nav-${item.id}`}
              onClick={() => setCurrentRoute(item.id)}
              title={isCollapsed ? item.label : undefined}
              className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-[8px] text-xs font-medium transition-all relative group ${
                isActive
                  ? 'bg-[#6B8C42]/10 text-[#E8D9A8] border-l-2 border-[#8AB55A] pl-2.5'
                  : 'text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08]'
              }`}
            >
              <Icon
                className={`w-4 h-4 shrink-0 transition-colors ${
                  isActive ? 'text-[#8AB55A]' : 'text-[#8A8577] group-hover:text-[#E8D9A8]'
                }`}
              />

              {!isCollapsed && <span className="truncate">{item.label}</span>}

              {/* Actionable Badges */}
              {!isCollapsed && item.badgeCount && item.badgeCount > 0 ? (
                <span className="ml-auto text-[10px] font-mono px-1.5 py-0.2 rounded-full bg-[#CC272E]/20 text-[#E8D9A8] border border-[#CC272E]/40 font-bold">
                  {item.badgeCount}
                </span>
              ) : null}

              {isCollapsed && item.badgeCount && item.badgeCount > 0 && (
                <span className="absolute top-1.5 right-1.5 w-2 h-2 rounded-full bg-[#CC272E]"></span>
              )}
            </button>
          );
        })}
      </nav>

      {/* Bottom Area: Runtime Status & User Account */}
      <div className="p-2 border-t border-[#23221C] space-y-2">
        {/* Runtime summary badge */}
        {!isCollapsed && (
          <div className="px-2.5 py-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between text-[11px] font-mono text-[#8A8577]">
            <span className="truncate">{currentWorkspace.runtimeVersion}</span>
            <span className="text-[#8AB55A] font-medium">OK</span>
          </div>
        )}

        {/* User / Account button */}
        <div className="relative">
          <button
            id="sidebar-user-menu-trigger"
            onClick={() => setIsUserMenuOpen(!isUserMenuOpen)}
            className="w-full flex items-center gap-2.5 p-2 rounded-[8px] hover:bg-[#0A0A08] transition-colors text-left"
          >
            <div className="w-7 h-7 rounded-full bg-[#23221C] border border-[#8A8577]/30 flex items-center justify-center text-xs font-mono font-bold text-[#E8D9A8] shrink-0">
              {userSession?.avatarInitials || 'LR'}
            </div>
            {!isCollapsed && (
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium text-[#E8D9A8] truncate">
                  {userSession?.name || 'Laukik Rajput'}
                </div>
                <div className="text-[10px] text-[#8A8577] truncate font-mono">
                  {userSession?.email || 'laukikxrajput@gmail.com'}
                </div>
              </div>
            )}
          </button>

          {/* Account Popover */}
          {isUserMenuOpen && (
            <div className="absolute bottom-full left-0 right-0 mb-2 bg-[#0F0F0C] border border-[#23221C] rounded-[10px] shadow-2xl p-1.5 z-50 text-xs space-y-1 font-sans">
              <div className="px-2.5 py-1.5 border-b border-[#23221C] mb-1">
                <div className="font-semibold text-[#E8D9A8]">{userSession?.name || 'Laukik Rajput'}</div>
                <div className="text-[10px] text-[#8A8577] font-mono">{userSession?.role || 'Cluster Admin'}</div>
              </div>
              <button
                id="open-account-settings-btn"
                onClick={() => {
                  setIsUserMenuOpen(false);
                  setIsAccountModalOpen(true);
                }}
                className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-[6px] text-[#E8D9A8] hover:bg-[#0A0A08] text-left"
              >
                <User className="w-3.5 h-3.5 text-[#8AB55A]" /> Account & Security
              </button>
              <button
                onClick={() => {
                  setIsUserMenuOpen(false);
                  setIsAccountModalOpen(true);
                }}
                className="w-full flex items-center justify-between px-2.5 py-1.5 rounded-[6px] text-[#E8D9A8] hover:bg-[#0A0A08] text-left"
              >
                <span className="flex items-center gap-2">
                  <Radio className="w-3.5 h-3.5 text-[#8A8577]" /> Runtime Discovery
                </span>
                <span className="text-[10px] text-[#8AB55A] font-mono">Active</span>
              </button>
              <div className="pt-1 border-t border-[#23221C]">
                <button
                  id="sidebar-signout-btn"
                  onClick={() => {
                    setIsUserMenuOpen(false);
                    setIsLogoutModalOpen(true);
                  }}
                  className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-[6px] text-[#CC272E] hover:bg-[#CC272E]/10 text-left font-semibold"
                >
                  <LogOut className="w-3.5 h-3.5" /> Sign Out
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    </aside>
  );
};
