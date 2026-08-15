import React, { useState } from 'react';
import { useLoki } from '../context/LokiContext';
import {
  Building,
  Bot,
  Radio,
  ShieldAlert,
  Activity,
  Bell,
  Key,
  HardDrive,
  CheckCircle2,
  Save,
} from 'lucide-react';

export const SettingsView: React.FC = () => {
  const { currentWorkspace, addToast } = useLoki();
  const [activeTab, setActiveTab] = useState<
    | 'workspace'
    | 'agents'
    | 'gateways'
    | 'permissions'
    | 'monitoring'
    | 'notifications'
    | 'security'
    | 'retention'
  >('workspace');

  const [workspaceName, setWorkspaceName] = useState(currentWorkspace.name);
  const [environment, setEnvironment] = useState(currentWorkspace.environment);

  const handleSave = () => {
    addToast({
      title: 'Configuration Saved',
      message: 'Workspace settings and security rules updated successfully.',
      type: 'success',
    });
  };

  const tabs = [
    { id: 'workspace', label: 'Workspace', icon: Building },
    { id: 'agents', label: 'Agent Integrations', icon: Bot },
    { id: 'gateways', label: 'Runtime & Gateways', icon: Radio },
    { id: 'permissions', label: 'Permissions & Approvals', icon: ShieldAlert },
    { id: 'monitoring', label: 'Monitoring Defaults', icon: Activity },
    { id: 'notifications', label: 'Alert Notifications', icon: Bell },
    { id: 'security', label: 'Security & Signing Keys', icon: Key },
    { id: 'retention', label: 'Data & Retention', icon: HardDrive },
  ] as const;

  return (
    <div id="settings-view" className="space-y-6">
      {/* Settings Navigation Layout */}
      <div className="grid grid-cols-1 md:grid-cols-4 gap-6">
        {/* Left Sub-nav */}
        <div className="space-y-1">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const isActive = activeTab === tab.id;
            return (
              <button
                key={tab.id}
                onClick={() => setActiveTab(tab.id)}
                className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-[8px] text-xs font-medium transition-all ${
                  isActive
                    ? 'bg-[#23221C] text-[#8AB55A] font-bold border-l-2 border-[#8AB55A]'
                    : 'text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#0A0A08]'
                }`}
              >
                <Icon className="w-4 h-4 shrink-0" />
                <span className="truncate">{tab.label}</span>
              </button>
            );
          })}
        </div>

        {/* Right Settings Content Form */}
        <div className="md:col-span-3 bg-[#050505] border border-[#23221C] rounded-[10px] p-6 space-y-6 text-xs text-[#E8D9A8]">
          {/* Tab 1: Workspace */}
          {activeTab === 'workspace' && (
            <div className="space-y-4">
              <div>
                <h3 className="text-sm font-bold text-[#E8D9A8] uppercase tracking-wider font-mono">
                  Workspace Profile & Environment
                </h3>
                <p className="text-xs text-[#8A8577] mt-0.5">
                  Configure hardware cluster name, target region, and deployment environment.
                </p>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-2 gap-4 font-mono">
                <div>
                  <label className="text-xs text-[#8A8577] uppercase block mb-1.5 font-sans font-semibold">
                    Workspace Name
                  </label>
                  <input
                    type="text"
                    value={workspaceName}
                    onChange={(e) => setWorkspaceName(e.target.value)}
                    className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                  />
                </div>

                <div>
                  <label className="text-xs text-[#8A8577] uppercase block mb-1.5 font-sans font-semibold">
                    Environment Target
                  </label>
                  <select
                    value={environment}
                    onChange={(e) => setEnvironment(e.target.value as any)}
                    className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                  >
                    <option value="production">Production Hardware Chamber</option>
                    <option value="staging">Staging / Test Bench</option>
                    <option value="development">Local Developer Lab</option>
                  </select>
                </div>
              </div>

              <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex justify-between items-center font-mono">
                <div>
                  <span className="text-[#E8D9A8] font-bold block">Connected Runtime Daemon</span>
                  <span className="text-[11px] text-[#8A8577]">
                    {currentWorkspace.runtimeVersion} • 0.0.0.0:3000
                  </span>
                </div>
                <span className="text-xs text-[#8AB55A] font-bold">ACTIVE</span>
              </div>
            </div>
          )}

          {/* Tab 2: Agent Integrations */}
          {activeTab === 'agents' && (
            <div className="space-y-4">
              <div>
                <h3 className="text-sm font-bold text-[#E8D9A8] uppercase tracking-wider font-mono">
                  Autonomous Coding Agent Bridges
                </h3>
                <p className="text-xs text-[#8A8577] mt-0.5">
                  Manage API tokens, tool permissions, and auto-approval limits for connected LLMs.
                </p>
              </div>

              <div className="space-y-3 font-mono">
                {[
                  {
                    name: 'Claude Code CLI Bridge',
                    status: 'Connected',
                    token: 'sk-ant-api03-verified-active',
                  },
                  {
                    name: 'OpenAI Codex Specialist',
                    status: 'Connected',
                    token: 'sk-proj-loki-active',
                  },
                  {
                    name: 'Cursor Agent RPC Connector',
                    status: 'Connected',
                    token: 'cur_bridge_local_token',
                  },
                ].map((item, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between"
                  >
                    <div>
                      <div className="font-bold text-[#E8D9A8]">{item.name}</div>
                      <div className="text-[10px] text-[#8A8577]">{item.token}</div>
                    </div>
                    <span className="text-[11px] px-2 py-0.5 rounded bg-[#6B8C42]/20 text-[#8AB55A] font-bold">
                      {item.status}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Tab 4: Permissions & Approvals */}
          {activeTab === 'permissions' && (
            <div className="space-y-4">
              <div>
                <h3 className="text-sm font-bold text-[#E8D9A8] uppercase tracking-wider font-mono">
                  Security Approvals & High-Risk Guardrails
                </h3>
                <p className="text-xs text-[#8A8577] mt-0.5">
                  All destructive or invasive hardware operations require human verification.
                </p>
              </div>

              <div className="space-y-2.5 font-mono">
                {[
                  {
                    op: 'Flash Firmware Release',
                    risk: 'Critical',
                    req: 'Operator Approval Required',
                  },
                  { op: 'Hardware Safe Reboot', risk: 'Medium', req: 'Auto-approved in Staging' },
                  {
                    op: 'Clear Non-Volatile Flash / NVS',
                    risk: 'Critical',
                    req: 'Operator Approval Required',
                  },
                  {
                    op: 'Direct Register Write (I2C/SPI)',
                    risk: 'High',
                    req: 'Operator Approval Required',
                  },
                ].map((item, idx) => (
                  <div
                    key={idx}
                    className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between"
                  >
                    <div>
                      <div className="font-bold text-[#E8D9A8]">{item.op}</div>
                      <div className="text-[10px] text-[#8A8577]">Risk Tier: {item.risk}</div>
                    </div>
                    <span className="text-[10px] px-2 py-0.5 rounded bg-[#23221C] text-[#8AB55A]">
                      {item.req}
                    </span>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Fallback for other tabs */}
          {activeTab !== 'workspace' && activeTab !== 'agents' && activeTab !== 'permissions' && (
            <div className="py-8 text-center font-mono space-y-2">
              <CheckCircle2 className="w-8 h-8 text-[#8AB55A] mx-auto" />
              <div className="text-sm font-bold text-[#E8D9A8]">Configuration Policy Active</div>
              <p className="text-xs text-[#8A8577] max-w-md mx-auto">
                Cluster policies, heartbeat timeouts, retention schedules, and notification webhooks
                are currently enforced by Loki Runtime v1.2.
              </p>
            </div>
          )}

          {/* Save Action */}
          <div className="pt-4 border-t border-[#23221C] flex justify-end">
            <button
              onClick={handleSave}
              className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors"
            >
              <Save className="w-4 h-4" /> Save Changes
            </button>
          </div>
        </div>
      </div>
    </div>
  );
};
