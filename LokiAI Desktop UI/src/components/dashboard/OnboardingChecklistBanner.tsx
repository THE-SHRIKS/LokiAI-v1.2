import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { Circle, X, ArrowRight, Bot, Cpu, ShieldCheck, Activity, Sliders } from 'lucide-react';

export const OnboardingChecklistBanner: React.FC = () => {
  const {
    onboardingState,
    dismissOnboardingChecklist,
    navigateTo,
    devices,
    agentRuns,
    monitoringRules,
    setIsAddDeviceModalOpen,
  } = useLoki();

  if (onboardingState.checklistDismissed) return null;

  // Determine completed tasks
  const hasConnectedAgent = onboardingState.connectedAgents.length > 0 || agentRuns.length > 0;
  const hasDevice = devices.length > 0;
  const hasHealthCheck = devices.some((d) => d.healthChecks && d.healthChecks.length > 0);
  const hasConfiguredPolicy = true; // Enabled by default in Chamber & Olive template
  const hasReviewedMonitoring = monitoringRules.length > 0;

  const checklistItems = [
    {
      id: 'agent',
      label: 'Connect a coding agent',
      completed: hasConnectedAgent,
      action: () => navigateTo('agents'),
      icon: Bot,
    },
    {
      id: 'device',
      label: 'Add hardware device',
      completed: hasDevice,
      action: () => setIsAddDeviceModalOpen(true),
      icon: Cpu,
    },
    {
      id: 'health',
      label: 'Complete initial health check',
      completed: hasHealthCheck,
      action: () => navigateTo('devices'),
      icon: ShieldCheck,
    },
    {
      id: 'monitoring',
      label: 'Review monitoring defaults & watchers',
      completed: hasReviewedMonitoring,
      action: () => navigateTo('monitoring'),
      icon: Activity,
    },
    {
      id: 'policy',
      label: 'Configure human approval policy',
      completed: hasConfiguredPolicy,
      action: () => navigateTo('settings', undefined, 'permissions'),
      icon: Sliders,
    },
  ];

  const pendingItems = checklistItems.filter((item) => !item.completed);

  // If all tasks are completed, do not render
  if (pendingItems.length === 0) return null;

  return (
    <div
      id="onboarding-first-checklist"
      className="p-4 rounded-[10px] bg-[#050505] border border-[#6B8C42]/40 text-xs font-sans space-y-3 animate-in fade-in"
    >
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <div className="w-2 h-2 rounded-full bg-[#8AB55A]"></div>
          <span className="font-bold text-[#E8D9A8]">Workspace Setup Checklist</span>
          <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A]">
            {checklistItems.length - pendingItems.length}/{checklistItems.length} complete
          </span>
        </div>

        <button
          onClick={dismissOnboardingChecklist}
          className="text-[#8A8577] hover:text-[#E8D9A8] p-1 rounded hover:bg-[#23221C] transition-colors"
          title="Dismiss Checklist"
        >
          <X className="w-3.5 h-3.5" />
        </button>
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-2">
        {pendingItems.map((item) => {
          return (
            <button
              key={item.id}
              onClick={item.action}
              className="p-2.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 flex items-center justify-between text-left group transition-all"
            >
              <div className="flex items-center gap-2">
                <Circle className="w-3.5 h-3.5 text-[#8A8577] group-hover:text-[#8AB55A]" />
                <span className="text-[#E8D9A8] group-hover:text-[#8AB55A] font-medium">
                  {item.label}
                </span>
              </div>
              <ArrowRight className="w-3.5 h-3.5 text-[#8A8577] group-hover:text-[#8AB55A] group-hover:translate-x-0.5 transition-transform" />
            </button>
          );
        })}
      </div>
    </div>
  );
};
