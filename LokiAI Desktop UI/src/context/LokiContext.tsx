import React, { createContext, useContext, useState, useEffect, useCallback, ReactNode } from 'react';
import {
  PrimaryRoute,
  AuthRoute,
  OnboardingRoute,
  AppRoute,
  AuthStatus,
  UserSession,
  OnboardingState,
  SettingsTab,
  Workspace,
  Device,
  AgentRun,
  Incident,
  Deployment,
  MonitoringRule,
  WatcherHealth,
  TelemetryPoint,
  LogEntry,
  ActivityEvent,
  ApprovalRequest,
} from '../types';
import {
  mockWorkspaces,
  mockDevices,
  mockAgentRuns,
  mockIncidents,
  mockDeployments,
  mockMonitoringRules,
  mockWatchers,
  mockTelemetryHistory,
  mockBoundedLogs,
  mockApprovals,
  mockActivityEvents,
} from '../data/mockData';

export interface ToastMessage {
  id: string;
  type: 'info' | 'success' | 'warning' | 'error';
  title: string;
  message: string;
  jobId?: string;
  linkRoute?: PrimaryRoute;
}

const defaultSession: UserSession = {
  id: 'usr-849201',
  name: 'Laukik Rajput',
  email: 'laukikxrajput@gmail.com',
  avatarInitials: 'LR',
  role: 'Cluster Admin (Full Hardware Ops)',
  organization: 'Loki Robotics Lab',
  plan: 'Enterprise Fleet Capacity',
  tokenExpiresAt: 'In 30 days',
  authenticatedAt: 'Just now',
};

const defaultOnboardingState: OnboardingState = {
  completed: false,
  currentStep: 'workspace',
  workspace: {
    name: 'Chamber-Alpha',
    description: 'Hardware node test bench and active validation chamber',
    environment: 'development',
    repoPath: '~/firmware/loki-chamber-core',
    connectRepoLater: false,
    isExisting: false,
  },
  connectedAgents: [
    {
      provider: 'Claude Code',
      name: 'Claude Code (MCP)',
      model: 'claude-3-7-sonnet',
      version: 'v2.4.1',
      status: 'connected',
      capabilities: [
        'Read Real-time Telemetry & Logs',
        'Trigger Diagnostic Pre-flights',
        'Investigate Hardware Incidents',
        'OTA Deployment (Requires Human Approval)',
      ],
      lastHandshake: 'Just now',
    },
  ],
  deviceChoice: 'existing',
  discoveredDevice: null,
  scratchProject: {
    repoPath: '~/projects/firmware-core',
    targetFamily: 'ESP32-S3',
    intendedConnector: 'USB-CDC Serial',
    decidedLater: false,
  },
  checklistDismissed: false,
};

interface LokiContextType {
  // Authentication & Session
  authStatus: AuthStatus;
  userSession: UserSession | null;
  startBrowserAuth: () => void;
  completeBrowserAuth: (customUser?: Partial<UserSession>) => void;
  cancelBrowserAuth: () => void;
  isLogoutModalOpen: boolean;
  setIsLogoutModalOpen: (open: boolean) => void;
  isAccountModalOpen: boolean;
  setIsAccountModalOpen: (open: boolean) => void;
  confirmLogout: () => void;

  // Onboarding
  onboardingState: OnboardingState;
  updateOnboardingState: (updates: Partial<OnboardingState>) => void;
  finishOnboarding: () => void;
  dismissOnboardingChecklist: () => void;

  // Unified Navigation & Routing
  activeAppRoute: AppRoute;
  navigateToRoute: (route: AppRoute, entityId?: string, tab?: SettingsTab) => void;

  // Navigation & Workspace
  currentWorkspace: Workspace;
  workspaces: Workspace[];
  setWorkspace: (ws: Workspace) => void;
  currentRoute: PrimaryRoute;
  setCurrentRoute: (route: PrimaryRoute) => void;
  currentSettingsTab: SettingsTab;
  setCurrentSettingsTab: (tab: SettingsTab) => void;
  navigateTo: (route: PrimaryRoute, entityId?: string, tab?: SettingsTab) => void;

  // Data Collections
  devices: Device[];
  agentRuns: AgentRun[];
  incidents: Incident[];
  deployments: Deployment[];
  monitoringRules: MonitoringRule[];
  watchers: WatcherHealth[];
  telemetryHistory: TelemetryPoint[];
  logs: LogEntry[];
  approvals: ApprovalRequest[];
  activityEvents: ActivityEvent[];

  // Selected Entities for detail views
  selectedDeviceId: string | null;
  setSelectedDeviceId: (id: string | null) => void;
  selectedAgentRunId: string | null;
  setSelectedAgentRunId: (id: string | null) => void;
  selectedIncidentId: string | null;
  setSelectedIncidentId: (id: string | null) => void;
  selectedDeploymentId: string | null;
  setSelectedDeploymentId: (id: string | null) => void;
  selectedActivityId: string | null;
  setSelectedActivityId: (id: string | null) => void;

  // Inspector Drawer
  inspectorOpen: boolean;
  inspectorType: 'device' | 'agent' | 'incident' | 'deployment' | 'activity' | 'rule' | null;
  inspectorData: any;
  openInspector: (type: 'device' | 'agent' | 'incident' | 'deployment' | 'activity' | 'rule', data: any) => void;
  closeInspector: () => void;

  // Modals
  isCommandPaletteOpen: boolean;
  setIsCommandPaletteOpen: (open: boolean) => void;
  activeApprovalModal: ApprovalRequest | null;
  setActiveApprovalModal: (appr: ApprovalRequest | null) => void;
  isDeployModalOpen: boolean;
  setIsDeployModalOpen: (open: boolean) => void;
  isAddDeviceModalOpen: boolean;
  setIsAddDeviceModalOpen: (open: boolean) => void;

  // Operations & Actions
  approveRequest: (approvalId: string, justification?: string) => void;
  denyRequest: (approvalId: string, reason?: string) => void;
  rebootDevice: (deviceId: string) => void;
  triggerPreflight: (deviceId: string) => void;
  triggerRollback: (deviceId: string) => void;
  resolveIncident: (incidentId: string) => boolean;
  startInvestigation: (incidentId: string) => void;
  toggleRule: (ruleId: string) => void;
  createDeployment: (newDep: Partial<Deployment>) => void;
  addDiscoveredDevice: (device: Partial<Device>) => void;

  // Toasts
  toasts: ToastMessage[];
  addToast: (toast: Omit<ToastMessage, 'id'>) => void;
  removeToast: (id: string) => void;

  // Live Stream Control
  isLiveStreaming: boolean;
  setIsLiveStreaming: (live: boolean) => void;
  lastSyncTime: string;
}

const LokiContext = createContext<LokiContextType | undefined>(undefined);

export const LokiProvider: React.FC<{ children: ReactNode }> = ({ children }) => {
  // Persistent Auth and Onboarding Storage
  const [authStatus, setAuthStatus] = useState<AuthStatus>(() => {
    try {
      const savedAuth = localStorage.getItem('loki_auth_session');
      return savedAuth ? 'authenticated' : 'unauthenticated';
    } catch {
      return 'unauthenticated';
    }
  });

  const [userSession, setUserSession] = useState<UserSession | null>(() => {
    try {
      const savedAuth = localStorage.getItem('loki_auth_session');
      return savedAuth ? JSON.parse(savedAuth) : null;
    } catch {
      return null;
    }
  });

  const [onboardingState, setOnboardingState] = useState<OnboardingState>(() => {
    try {
      const savedOnboarding = localStorage.getItem('loki_onboarding_state');
      return savedOnboarding ? JSON.parse(savedOnboarding) : defaultOnboardingState;
    } catch {
      return defaultOnboardingState;
    }
  });

  const [activeAppRoute, setActiveAppRoute] = useState<AppRoute>(() => {
    try {
      const savedAuth = localStorage.getItem('loki_auth_session');
      const savedOnboarding = localStorage.getItem('loki_onboarding_state');
      if (!savedAuth) return 'auth';
      if (savedOnboarding) {
        const parsed = JSON.parse(savedOnboarding);
        if (parsed.completed) return 'dashboard';
        if (parsed.currentStep === 'workspace') return 'onboarding/workspace';
        if (parsed.currentStep === 'agent') return 'onboarding/agent';
        if (parsed.currentStep === 'device') return 'onboarding/device-choice';
        if (parsed.currentStep === 'ready') return 'onboarding/ready';
      }
      return 'auth';
    } catch {
      return 'auth';
    }
  });

  // Modal states for Logout & Account
  const [isLogoutModalOpen, setIsLogoutModalOpen] = useState(false);
  const [isAccountModalOpen, setIsAccountModalOpen] = useState(false);

  // Sync to localStorage
  useEffect(() => {
    try {
      if (userSession && authStatus === 'authenticated') {
        localStorage.setItem('loki_auth_session', JSON.stringify(userSession));
      } else {
        localStorage.removeItem('loki_auth_session');
      }
    } catch (e) {
      console.warn('Could not persist auth session', e);
    }
  }, [userSession, authStatus]);

  useEffect(() => {
    try {
      localStorage.setItem('loki_onboarding_state', JSON.stringify(onboardingState));
    } catch (e) {
      console.warn('Could not persist onboarding state', e);
    }
  }, [onboardingState]);

  // Workspaces & Routing
  const [workspaces, setWorkspaces] = useState<Workspace[]>(mockWorkspaces);
  const [currentWorkspace, setCurrentWorkspace] = useState<Workspace>(mockWorkspaces[0]);
  const [currentRoute, setCurrentRoute] = useState<PrimaryRoute>('dashboard');
  const [currentSettingsTab, setCurrentSettingsTab] = useState<SettingsTab>('workspace');

  // Entities
  const [devices, setDevices] = useState<Device[]>(mockDevices);
  const [agentRuns, setAgentRuns] = useState<AgentRun[]>(mockAgentRuns);
  const [incidents, setIncidents] = useState<Incident[]>(mockIncidents);
  const [deployments, setDeployments] = useState<Deployment[]>(mockDeployments);
  const [monitoringRules, setMonitoringRules] = useState<MonitoringRule[]>(mockMonitoringRules);
  const [watchers, setWatchers] = useState<WatcherHealth[]>(mockWatchers);
  const [telemetryHistory, setTelemetryHistory] = useState<TelemetryPoint[]>(mockTelemetryHistory);
  const [logs, setLogs] = useState<LogEntry[]>(mockBoundedLogs);
  const [approvals, setApprovals] = useState<ApprovalRequest[]>(mockApprovals);
  const [activityEvents, setActivityEvents] = useState<ActivityEvent[]>(mockActivityEvents);

  // Detail Selections
  const [selectedDeviceId, setSelectedDeviceId] = useState<string | null>(null);
  const [selectedAgentRunId, setSelectedAgentRunId] = useState<string | null>(null);
  const [selectedIncidentId, setSelectedIncidentId] = useState<string | null>(null);
  const [selectedDeploymentId, setSelectedDeploymentId] = useState<string | null>(null);
  const [selectedActivityId, setSelectedActivityId] = useState<string | null>(null);

  // Inspector Drawer
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [inspectorType, setInspectorType] = useState<'device' | 'agent' | 'incident' | 'deployment' | 'activity' | 'rule' | null>(null);
  const [inspectorData, setInspectorData] = useState<any>(null);

  // Modals
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
  const [activeApprovalModal, setActiveApprovalModal] = useState<ApprovalRequest | null>(null);
  const [isDeployModalOpen, setIsDeployModalOpen] = useState(false);
  const [isAddDeviceModalOpen, setIsAddDeviceModalOpen] = useState(false);

  // Toasts
  const [toasts, setToasts] = useState<ToastMessage[]>([]);

  // Live stream state
  const [isLiveStreaming, setIsLiveStreaming] = useState(true);
  const [lastSyncTime, setLastSyncTime] = useState('Just now');

  const addToast = useCallback((toast: Omit<ToastMessage, 'id'>) => {
    const id = `toast-${Date.now()}-${Math.random().toString(36).substr(2, 4)}`;
    setToasts((prev) => [...prev, { ...toast, id }]);
    setTimeout(() => {
      setToasts((prev) => prev.filter((t) => t.id !== id));
    }, 6000);
  }, []);

  const removeToast = useCallback((id: string) => {
    setToasts((prev) => prev.filter((t) => t.id !== id));
  }, []);

  const openInspector = useCallback(
    (type: 'device' | 'agent' | 'incident' | 'deployment' | 'activity' | 'rule', data: any) => {
      setInspectorType(type);
      setInspectorData(data);
      setInspectorOpen(true);
    },
    []
  );

  const closeInspector = useCallback(() => {
    setInspectorOpen(false);
    setInspectorType(null);
    setInspectorData(null);
  }, []);

  // Unified Navigation Action
  const navigateToRoute = useCallback(
    (route: AppRoute, entityId?: string, tab?: SettingsTab) => {
      setActiveAppRoute(route);

      // If it's a primary dashboard route, keep currentRoute in sync
      const dashboardRoutes: PrimaryRoute[] = [
        'dashboard',
        'devices',
        'agents',
        'incidents',
        'deployments',
        'monitoring',
        'settings',
        'activity',
      ];
      if (dashboardRoutes.includes(route as PrimaryRoute)) {
        setCurrentRoute(route as PrimaryRoute);
        if (tab) setCurrentSettingsTab(tab);
        if (entityId) {
          if (route === 'devices') setSelectedDeviceId(entityId);
          if (route === 'agents') setSelectedAgentRunId(entityId);
          if (route === 'incidents') setSelectedIncidentId(entityId);
          if (route === 'deployments') setSelectedDeploymentId(entityId);
          if (route === 'activity') setSelectedActivityId(entityId);
        }
      }
    },
    []
  );

  const navigateTo = useCallback(
    (route: PrimaryRoute, entityId?: string, tab?: SettingsTab) => {
      setActiveAppRoute(route);
      setCurrentRoute(route);
      if (tab) setCurrentSettingsTab(tab);
      if (entityId) {
        if (route === 'devices') setSelectedDeviceId(entityId);
        if (route === 'agents') setSelectedAgentRunId(entityId);
        if (route === 'incidents') setSelectedIncidentId(entityId);
        if (route === 'deployments') setSelectedDeploymentId(entityId);
        if (route === 'activity') setSelectedActivityId(entityId);
      }
    },
    []
  );

  // Auth Operations
  const startBrowserAuth = () => {
    setAuthStatus('waiting');
    setActiveAppRoute('auth/waiting');
  };

  const completeBrowserAuth = (customUser?: Partial<UserSession>) => {
    const session = customUser ? { ...defaultSession, ...customUser } : defaultSession;
    setUserSession(session);
    setAuthStatus('authenticated');

    if (onboardingState.completed) {
      setActiveAppRoute('dashboard');
      setCurrentRoute('dashboard');
    } else {
      if (onboardingState.currentStep === 'workspace') {
        setActiveAppRoute('onboarding/workspace');
      } else if (onboardingState.currentStep === 'agent') {
        setActiveAppRoute('onboarding/agent');
      } else if (onboardingState.currentStep === 'device') {
        setActiveAppRoute('onboarding/device-choice');
      } else {
        setActiveAppRoute('onboarding/ready');
      }
    }
  };

  const cancelBrowserAuth = () => {
    setAuthStatus('unauthenticated');
    setActiveAppRoute('auth');
  };

  const confirmLogout = () => {
    setUserSession(null);
    setAuthStatus('unauthenticated');
    setActiveAppRoute('auth');
    setIsLogoutModalOpen(false);
    setIsAccountModalOpen(false);
    try {
      localStorage.removeItem('loki_auth_session');
    } catch {}
    addToast({
      type: 'info',
      title: 'Signed Out',
      message: 'Hardware control session closed securely.',
    });
  };

  const updateOnboardingState = (updates: Partial<OnboardingState>) => {
    setOnboardingState((prev) => ({ ...prev, ...updates }));
  };

  const finishOnboarding = () => {
    setOnboardingState((prev) => ({ ...prev, completed: true, currentStep: 'ready' }));
    setActiveAppRoute('dashboard');
    setCurrentRoute('dashboard');
  };

  const dismissOnboardingChecklist = () => {
    setOnboardingState((prev) => ({ ...prev, checklistDismissed: true }));
  };

  // Live Telemetry Simulation Tick
  useEffect(() => {
    if (!isLiveStreaming) return;
    const interval = setInterval(() => {
      setLastSyncTime('Just now');
      const now = new Date();
      const timeLabel = `${String(now.getUTCHours()).padStart(2, '0')}:${String(now.getUTCMinutes()).padStart(2, '0')}:${String(now.getUTCSeconds()).padStart(2, '0')}`;

      // Simulate subtle telemetry variation
      const latestVoltage = Number((3.30 + (Math.random() * 0.04 - 0.02)).toFixed(2));
      const latestTemp = Number((48.0 + (Math.random() * 0.6 - 0.3)).toFixed(1));
      const latestCpu = Number((34.0 + (Math.random() * 3.0 - 1.5)).toFixed(1));

      const newPoint: TelemetryPoint = {
        timestamp: now.toISOString(),
        timeLabel,
        voltage: latestVoltage,
        temperature: latestTemp,
        cpuUsage: latestCpu,
        heapFreeKB: 214,
        packetLossPct: 0.0,
        watchdogMarginMs: 388,
      };

      setTelemetryHistory((prev) => {
        const next = [...prev.slice(1), newPoint];
        return next;
      });

      // Update observation countdown on active deployments
      setDeployments((prev) =>
        prev.map((dep) => {
          if (dep.phase === 'observing' && dep.observationElapsedSec < dep.observationWindowSec) {
            const nextElapsed = dep.observationElapsedSec + 2;
            if (nextElapsed >= dep.observationWindowSec) {
              return {
                ...dep,
                phase: 'verified',
                observationElapsedSec: dep.observationWindowSec,
                completedTime: 'Just now',
                deviceResults: dep.deviceResults.map((dr) => ({
                  ...dr,
                  status: 'Completed',
                  progressPct: 100,
                  currentStage: 'Completed & Verified',
                })),
              };
            }
            return {
              ...dep,
              observationElapsedSec: nextElapsed,
              deviceResults: dep.deviceResults.map((dr) => ({
                ...dr,
                progressPct: Math.min(98, Math.round((nextElapsed / dep.observationWindowSec) * 100)),
                currentStage: `Observing: ${nextElapsed}/${dep.observationWindowSec}s elapsed`,
              })),
            };
          }
          return dep;
        })
      );
    }, 3000);

    return () => clearInterval(interval);
  }, [isLiveStreaming]);

  // Operational Actions
  const approveRequest = (approvalId: string, justification?: string) => {
    const targetApproval = approvals.find((a) => a.id === approvalId);
    if (!targetApproval) return;

    // Update approval state
    setApprovals((prev) =>
      prev.map((appr) => (appr.id === approvalId ? { ...appr, status: 'approved' } : appr))
    );

    // If associated with an agent run, advance agent run
    if (targetApproval.associatedAgentRunId) {
      setAgentRuns((prev) =>
        prev.map((run) => {
          if (run.id === targetApproval.associatedAgentRunId) {
            return {
              ...run,
              phase: 'deploying',
              lastEvent: 'Approval granted by user. Executing firmware flash job...',
              requestedOperations: run.requestedOperations.map((op) =>
                op.approvalId === approvalId
                  ? { ...op, state: 'executing', executionJobId: `job-${Date.now()}` }
                  : op
              ),
            };
          }
          return run;
        })
      );
    }

    // Add activity record
    const newAct: ActivityEvent = {
      id: `act-${Date.now()}`,
      timestamp: 'Just now',
      eventType: 'APPROVAL_GRANTED',
      actor: { name: 'Laukik Rajput', type: 'user' },
      workspace: currentWorkspace.name,
      resourceType: 'Device',
      resourceId: targetApproval.targetDeviceIds[0] || 'device-01',
      resourceName: targetApproval.targetDeviceNames[0] || 'Target Device',
      actionSummary: `Approved operation: ${targetApproval.operationName} (${targetApproval.reason})`,
      result: 'Approved',
      riskLevel: targetApproval.risk,
      correlation: {
        incidentId: targetApproval.associatedIncidentId,
        agentRunId: targetApproval.associatedAgentRunId,
        jobId: `job-appr-${approvalId}`,
      },
      detailsPayload: { justification: justification || 'Authorized via Approval Modal' },
    };
    setActivityEvents((prev) => [newAct, ...prev]);

    // Close modal & toast
    setActiveApprovalModal(null);
    addToast({
      type: 'success',
      title: 'Operation Approved & Dispatched',
      message: `Authorized ${targetApproval.operationName} on ${targetApproval.targetDeviceNames.join(', ')}. Job ID: job-appr-${approvalId}`,
      jobId: `job-appr-${approvalId}`,
    });
  };

  const denyRequest = (approvalId: string, reason?: string) => {
    const targetApproval = approvals.find((a) => a.id === approvalId);
    if (!targetApproval) return;

    setApprovals((prev) =>
      prev.map((appr) => (appr.id === approvalId ? { ...appr, status: 'denied' } : appr))
    );

    if (targetApproval.associatedAgentRunId) {
      setAgentRuns((prev) =>
        prev.map((run) => {
          if (run.id === targetApproval.associatedAgentRunId) {
            return {
              ...run,
              phase: 'waiting',
              lastEvent: 'Operation denied by user. Agent awaiting revised instructions.',
              requestedOperations: run.requestedOperations.map((op) =>
                op.approvalId === approvalId ? { ...op, state: 'rejected' } : op
              ),
            };
          }
          return run;
        })
      );
    }

    const newAct: ActivityEvent = {
      id: `act-${Date.now()}`,
      timestamp: 'Just now',
      eventType: 'APPROVAL_DENIED',
      actor: { name: 'Laukik Rajput', type: 'user' },
      workspace: currentWorkspace.name,
      resourceType: 'Device',
      resourceId: targetApproval.targetDeviceIds[0] || 'device-01',
      resourceName: targetApproval.targetDeviceNames[0] || 'Target Device',
      actionSummary: `Denied operation: ${targetApproval.operationName}`,
      result: 'Denied',
      riskLevel: targetApproval.risk,
      detailsPayload: { reason: reason || 'Operation denied by operator' },
      correlation: {
        incidentId: targetApproval.associatedIncidentId,
        agentRunId: targetApproval.associatedAgentRunId,
      },
    };
    setActivityEvents((prev) => [newAct, ...prev]);

    setActiveApprovalModal(null);
    addToast({
      type: 'warning',
      title: 'Operation Denied',
      message: `Denied ${targetApproval.operationName} on ${targetApproval.targetDeviceNames.join(', ')}.`,
    });
  };

  const rebootDevice = (deviceId: string) => {
    const dev = devices.find((d) => d.id === deviceId);
    if (!dev) return;

    addToast({
      type: 'info',
      title: 'Reboot Signal Sent',
      message: `Issued hardware soft-reset via ${dev.transportPort} to ${dev.name}. Job ID: job-reboot-${Date.now()}`,
      jobId: `job-reboot-${Date.now()}`,
    });

    const newAct: ActivityEvent = {
      id: `act-${Date.now()}`,
      timestamp: 'Just now',
      eventType: 'DEVICE_REBOOT',
      actor: { name: 'Laukik Rajput', type: 'user' },
      workspace: currentWorkspace.name,
      resourceType: 'Device',
      resourceId: dev.id,
      resourceName: dev.name,
      actionSummary: `Safe reboot initiated via ${dev.transportPort}`,
      result: 'Success',
      riskLevel: 'Medium',
      correlation: { jobId: `job-reboot-${Date.now()}` },
    };
    setActivityEvents((prev) => [newAct, ...prev]);
  };

  const triggerPreflight = (deviceId: string) => {
    const dev = devices.find((d) => d.id === deviceId);
    if (!dev) return;

    addToast({
      type: 'info',
      title: 'Preflight Suite Dispatched',
      message: `Executing hardware diagnostic probe on ${dev.name}. Job ID: job-pf-${Date.now()}`,
      jobId: `job-pf-${Date.now()}`,
    });
  };

  const triggerRollback = (deviceId: string) => {
    const dev = devices.find((d) => d.id === deviceId);
    if (!dev || !dev.rollbackEligible) return;

    addToast({
      type: 'warning',
      title: 'Rollback Initiated',
      message: `Restoring previous verified artifact (${dev.previousFirmwareVersion || 'v1.4.0-stable'}) to ${dev.name}. Job ID: job-rollback-${Date.now()}`,
      jobId: `job-rollback-${Date.now()}`,
    });
  };

  const resolveIncident = (incidentId: string): boolean => {
    const inc = incidents.find((i) => i.id === incidentId);
    if (!inc) return false;

    // Rule 10: Never mark an incident resolved until a recorded verification has passed.
    if (!inc.verification.allPassed) {
      addToast({
        type: 'error',
        title: 'Resolution Blocked',
        message: `Cannot resolve ${inc.id}: Recorded verification criteria have not fully passed yet.`,
      });
      return false;
    }

    setIncidents((prev) =>
      prev.map((item) => (item.id === incidentId ? { ...item, status: 'Resolved' } : item))
    );

    const newAct: ActivityEvent = {
      id: `act-${Date.now()}`,
      timestamp: 'Just now',
      eventType: 'INCIDENT_RESOLVED',
      actor: { name: 'Laukik Rajput', type: 'user' },
      workspace: currentWorkspace.name,
      resourceType: 'Incident',
      resourceId: inc.id,
      resourceName: inc.id,
      actionSummary: `Incident marked Resolved after recorded verification validation`,
      result: 'Success',
      riskLevel: 'Medium',
      correlation: { incidentId: inc.id },
    };
    setActivityEvents((prev) => [newAct, ...prev]);

    addToast({
      type: 'success',
      title: 'Incident Resolved',
      message: `${inc.id} verified and resolved. Recorded in audit activity.`,
    });
    return true;
  };

  const startInvestigation = (incidentId: string) => {
    const inc = incidents.find((i) => i.id === incidentId);
    if (!inc) return;

    addToast({
      type: 'info',
      title: 'Agent Dispatched',
      message: `Claude Code assigned to investigate evidence for ${inc.id}. Job ID: job-agent-${Date.now()}`,
      jobId: `job-agent-${Date.now()}`,
    });
  };

  const toggleRule = (ruleId: string) => {
    setMonitoringRules((prev) =>
      prev.map((r) => (r.id === ruleId ? { ...r, enabled: !r.enabled } : r))
    );
    const rule = monitoringRules.find((r) => r.id === ruleId);
    if (rule) {
      addToast({
        type: 'info',
        title: rule.enabled ? 'Rule Disabled' : 'Rule Enabled',
        message: `${rule.name} is now ${rule.enabled ? 'inactive' : 'actively monitoring'}.`,
      });
    }
  };

  const createDeployment = (newDepData: Partial<Deployment>) => {
    const newDep: Deployment = {
      id: `dep-${Date.now().toString().slice(-4)}`,
      releaseTag: newDepData.releaseTag || 'v1.0.0-custom',
      artifactType: newDepData.artifactType || 'Firmware (.bin/.elf)',
      artifactVersion: newDepData.artifactVersion || 'v1.0.0',
      artifactDigestSha256: `sha256:${Array.from({ length: 64 }, () => Math.floor(Math.random() * 16).toString(16)).join('')}`,
      signatureStatus: 'Cryptographically Verified (Loki-Ed25519)',
      sourceRevision: 'commit 8a1f29b (main)',
      targetFleetScope: newDepData.targetFleetScope || 'Selected Devices (1)',
      targetDeviceIds: newDepData.targetDeviceIds || ['dev-esp32-01'],
      initiator: { name: 'Laukik Rajput', type: 'user', avatarLabel: 'LR' },
      rolloutStrategy: newDepData.rolloutStrategy || 'All-at-Once',
      phase: 'preflight',
      preflightChecks: [
        { id: 'pf-new-1', name: 'Binary Integrity Check', category: 'Binary', status: 'passed', measured: 'Valid Header', target: 'Valid' },
        { id: 'pf-new-2', name: 'Transport Connection Test', category: 'Transport', status: 'passed', measured: 'Ping < 5ms', target: '< 20ms' },
      ],
      deviceResults: (newDepData.targetDeviceIds || ['dev-esp32-01']).map((id) => ({
        deviceId: id,
        deviceName: devices.find((d) => d.id === id)?.name || id,
        status: 'Preflight',
        progressPct: 15,
        currentStage: 'Validating device preflight checks',
        firmwareApplied: newDepData.artifactVersion || 'v1.0.0',
      })),
      observationWindowSec: newDepData.observationWindowSec || 60,
      observationElapsedSec: 0,
      rollbackEligible: true,
      startTime: 'Just now',
      operationLogs: [
        `Just now [INFO] Initiated deployment ${newDepData.releaseTag}`,
        'Just now [INFO] Preflight checks initialized on target devices',
      ],
    };

    setDeployments((prev) => [newDep, ...prev]);
    setIsDeployModalOpen(false);
    addToast({
      type: 'info',
      title: 'Deployment Queued',
      message: `Deployment ${newDep.id} (${newDep.releaseTag}) started with Preflight checks.`,
      jobId: `job-${newDep.id}`,
      linkRoute: 'deployments',
    });
  };

  const addDiscoveredDevice = (deviceData: Partial<Device>) => {
    const newDev: Device = {
      id: `dev-${Date.now().toString().slice(-4)}`,
      name: deviceData.name || 'NEW-HARDWARE-NODE',
      hardwareId: deviceData.hardwareId || `HWID-${Date.now()}`,
      family: deviceData.family || 'ESP32-S3',
      manufacturer: deviceData.manufacturer || 'Custom Hardware',
      boardRevision: deviceData.boardRevision || 'Rev 1.0',
      supportTier: 'Tier-1 Certified',
      connector: deviceData.connector || 'USB-CDC Serial',
      transportPort: deviceData.transportPort || '/dev/ttyUSB1',
      gatewayId: 'gw-chamber-01',
      gatewayName: 'Chamber Core Gateway A',
      location: 'Rack 04 - Shelf 3',
      status: 'Healthy',
      lastSeen: 'Just now',
      heartbeatIntervalSec: 5,
      missedHeartbeats: 0,
      firmwareVersion: 'v1.0.0-factory',
      configVersion: 'default-config',
      appVersion: 'app-node-v1',
      capabilities: ['read_logs', 'restart', 'flash_firmware', 'ota_update', 'rollback', 'voltage_sense'],
      healthChecks: [
        {
          id: 'new-chk-1',
          name: 'Core Supply Rail',
          category: 'Power',
          status: 'Healthy',
          measuredValue: '3.30 V',
          threshold: '3.20V - 3.40V',
          lastChecked: 'Just now',
        },
      ],
      openIncidentCount: 0,
      activeDeploymentsCount: 0,
      rollbackEligible: false,
      metrics: {
        voltage: 3.30,
        temperature: 38.5,
        cpuUsage: 12.0,
        heapFreeKB: 250,
        packetLossPct: 0.0,
        watchdogMarginMs: 500,
      },
      relationshipNodes: [
        { id: 'gw-chamber-01', name: 'Chamber Core Gateway A', type: 'gateway', status: 'Healthy', details: 'Loki Runtime Socket v1.2.4' },
        { id: `dev-${Date.now().toString().slice(-4)}`, name: deviceData.name || 'NEW-HARDWARE-NODE', type: 'device', status: 'Healthy', details: 'Active Node' },
      ],
      relationshipEdges: [
        { source: 'gw-chamber-01', target: `dev-${Date.now().toString().slice(-4)}`, label: 'Transport Link', protocol: 'Loki Protocol' },
      ],
    };

    setDevices((prev) => [newDev, ...prev]);
    setIsAddDeviceModalOpen(false);
    addToast({
      type: 'success',
      title: 'Device Connected & Verified',
      message: `Registered ${newDev.name} (${newDev.family}) on ${newDev.transportPort}.`,
      linkRoute: 'devices',
    });
  };

  const setWorkspace = (ws: Workspace) => {
    setCurrentWorkspace(ws);
    addToast({
      type: 'info',
      title: 'Workspace Switched',
      message: `Active environment: ${ws.name} (${ws.environment})`,
    });
  };

  return (
    <LokiContext.Provider
      value={{
        authStatus,
        userSession,
        startBrowserAuth,
        completeBrowserAuth,
        cancelBrowserAuth,
        isLogoutModalOpen,
        setIsLogoutModalOpen,
        isAccountModalOpen,
        setIsAccountModalOpen,
        confirmLogout,
        onboardingState,
        updateOnboardingState,
        finishOnboarding,
        dismissOnboardingChecklist,
        activeAppRoute,
        navigateToRoute,
        currentWorkspace,
        workspaces,
        setWorkspace,
        currentRoute,
        setCurrentRoute,
        currentSettingsTab,
        setCurrentSettingsTab,
        navigateTo,
        devices,
        agentRuns,
        incidents,
        deployments,
        monitoringRules,
        watchers,
        telemetryHistory,
        logs,
        approvals,
        activityEvents,
        selectedDeviceId,
        setSelectedDeviceId,
        selectedAgentRunId,
        setSelectedAgentRunId,
        selectedIncidentId,
        setSelectedIncidentId,
        selectedDeploymentId,
        setSelectedDeploymentId,
        selectedActivityId,
        setSelectedActivityId,
        inspectorOpen,
        inspectorType,
        inspectorData,
        openInspector,
        closeInspector,
        isCommandPaletteOpen,
        setIsCommandPaletteOpen,
        activeApprovalModal,
        setActiveApprovalModal,
        isDeployModalOpen,
        setIsDeployModalOpen,
        isAddDeviceModalOpen,
        setIsAddDeviceModalOpen,
        approveRequest,
        denyRequest,
        rebootDevice,
        triggerPreflight,
        triggerRollback,
        resolveIncident,
        startInvestigation,
        toggleRule,
        createDeployment,
        addDiscoveredDevice,
        toasts,
        addToast,
        removeToast,
        isLiveStreaming,
        setIsLiveStreaming,
        lastSyncTime,
      }}
    >
      {children}
    </LokiContext.Provider>
  );
};

export const useLoki = (): LokiContextType => {
  const context = useContext(LokiContext);
  if (!context) {
    throw new Error('useLoki must be used within a LokiProvider');
  }
  return context;
};
