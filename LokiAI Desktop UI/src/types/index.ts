export type GlobalStatus =
  | 'Connected'
  | 'Disconnected'
  | 'Healthy'
  | 'Degraded'
  | 'Unhealthy'
  | 'Unknown'
  | 'Waiting'
  | 'Running'
  | 'Approval required'
  | 'Verifying'
  | 'Completed'
  | 'Failed'
  | 'Cancelled'
  | 'Rolled back'
  | 'Resolved';

export type IncidentSeverity = 'Critical' | 'High' | 'Medium' | 'Low';
export type IncidentStatus = 'Open' | 'Investigating' | 'Verifying' | 'Resolved' | 'Closed';

export type RiskLevel = 'Low' | 'Medium' | 'High' | 'Critical';

export type PrimaryRoute =
  | 'dashboard'
  | 'devices'
  | 'agents'
  | 'monitoring'
  | 'incidents'
  | 'deployments'
  | 'activity'
  | 'settings';

export type AuthRoute = 'auth' | 'auth/waiting';

export type OnboardingRoute =
  | 'onboarding/workspace'
  | 'onboarding/agent'
  | 'onboarding/device-choice'
  | 'onboarding/device-connect'
  | 'onboarding/device-health'
  | 'onboarding/project'
  | 'onboarding/ready';

export type AppRoute = AuthRoute | OnboardingRoute | PrimaryRoute;

export type AuthStatus = 'unauthenticated' | 'waiting' | 'authenticated';

export interface UserSession {
  id: string;
  name: string;
  email: string;
  avatarInitials: string;
  role: string;
  organization: string;
  plan: string;
  tokenExpiresAt: string;
  authenticatedAt: string;
}

export type OnboardingStepName = 'account' | 'workspace' | 'agent' | 'device' | 'ready';

export interface OnboardingState {
  completed: boolean;
  currentStep: OnboardingStepName;
  workspace: {
    name: string;
    description: string;
    environment: 'development' | 'staging' | 'production';
    repoPath: string;
    connectRepoLater: boolean;
    isExisting: boolean;
    existingWorkspaceId?: string;
  };
  connectedAgents: {
    provider: AgentProvider | 'Gravity';
    name: string;
    model: string;
    version: string;
    status: 'connected' | 'configuring' | 'failed';
    capabilities: string[];
    lastHandshake: string;
  }[];
  deviceChoice: 'existing' | 'scratch' | null;
  discoveredDevice: {
    id: string;
    name: string;
    family: DeviceFamily;
    connector: ConnectorType;
    transportPort: string;
    supportTier: string;
    healthStatus: 'Healthy' | 'Degraded' | 'Unhealthy';
    healthChecks: {
      id: string;
      name: string;
      description: string;
      status: 'passed' | 'failed' | 'warning' | 'pending' | 'running';
      details: string;
    }[];
  } | null;
  scratchProject: {
    repoPath: string;
    targetFamily: string;
    intendedConnector: string;
    decidedLater: boolean;
  };
  checklistDismissed: boolean;
}

export type SettingsTab =
  | 'workspace'
  | 'agents'
  | 'runtime'
  | 'permissions'
  | 'monitoring'
  | 'notifications'
  | 'security'
  | 'data';

export type DeviceFamily =
  | 'ESP32-S3'
  | 'STM32-H7'
  | 'nRF5340'
  | 'Raspberry Pi CM4'
  | 'i.MX8M-Plus'
  | 'RP2040-CAN'
  | 'Microchip SAM-E54';

export type ConnectorType =
  | 'USB-CDC Serial'
  | 'SWD / JTAG Probe'
  | 'BLE Gateway (mTLS)'
  | 'Industrial Ethernet / MQTT'
  | 'CAN-FD Bus'
  | 'I2C / SPI Bridge';

export type DeviceCapability =
  | 'read_logs'
  | 'restart'
  | 'flash_firmware'
  | 'ota_update'
  | 'run_preflight'
  | 'test_suite'
  | 'rollback'
  | 'can_inject'
  | 'voltage_sense';

export interface HealthCheckResult {
  id: string;
  name: string;
  category: 'Power' | 'Transport' | 'Compute' | 'Firmware' | 'Thermal';
  status: 'Healthy' | 'Degraded' | 'Unhealthy' | 'Unknown';
  measuredValue: string;
  threshold: string;
  lastChecked: string;
  details?: string;
}

export interface DeviceRelationshipNode {
  id: string;
  name: string;
  type: 'gateway' | 'device' | 'sensor' | 'bus' | 'project';
  connectionType?: string;
  status: 'Healthy' | 'Degraded' | 'Unhealthy' | 'Offline' | 'Active';
  details: string;
}

export interface DeviceRelationshipEdge {
  source: string;
  target: string;
  label: string;
  protocol: string;
}

export interface Device {
  id: string;
  name: string;
  hardwareId: string; // Serial / MAC / UID
  family: DeviceFamily;
  manufacturer: string;
  boardRevision: string;
  supportTier: 'Tier-1 Certified' | 'Tier-2 Supported' | 'Custom Transport';
  connector: ConnectorType;
  transportPort: string; // e.g. /dev/ttyUSB0, tcp://10.0.4.12:8883
  gatewayId: string;
  gatewayName: string;
  location: string;
  status: GlobalStatus;
  lastSeen: string;
  heartbeatIntervalSec: number;
  missedHeartbeats: number;
  firmwareVersion: string;
  configVersion: string;
  appVersion: string;
  modelArtifactVersion?: string;
  capabilities: DeviceCapability[];
  healthChecks: HealthCheckResult[];
  openIncidentCount: number;
  activeDeploymentsCount: number;
  rollbackEligible: boolean;
  previousFirmwareVersion?: string;
  relationshipNodes: DeviceRelationshipNode[];
  relationshipEdges: DeviceRelationshipEdge[];
  metrics: {
    voltage: number; // Volts
    temperature: number; // °C
    cpuUsage: number; // %
    heapFreeKB: number;
    packetLossPct: number;
    watchdogMarginMs: number;
  };
}

export type AgentProvider = 'Claude Code' | 'OpenAI Codex' | 'Cursor CLI' | 'Loki Agent Bridge';

export type AgentPhase =
  | 'investigating'
  | 'editing'
  | 'building'
  | 'requesting action'
  | 'deploying'
  | 'verifying'
  | 'waiting'
  | 'complete'
  | 'failed';

export interface TaskNode {
  id: string;
  label: string;
  agentRole: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  inputs: Record<string, string>;
  outputSummary?: string;
  elapsedSec: number;
  children?: string[];
}

export interface ToolRequest {
  id: string;
  operationName: string;
  targetDeviceId: string;
  targetDeviceName: string;
  inputs: Record<string, any>;
  risk: RiskLevel;
  state: 'pending_approval' | 'approved' | 'rejected' | 'executing' | 'completed' | 'failed';
  approvalId?: string;
  executionJobId?: string;
  resultSummary?: string;
  timestamp: string;
}

export interface FileChangeDiff {
  path: string;
  status: 'modified' | 'added' | 'deleted';
  additions: number;
  deletions: number;
  patchSummary: string;
}

export interface AgentRun {
  id: string;
  sessionId: string;
  provider: AgentProvider;
  taskTitle: string;
  operationalSummary: string;
  parentRunId?: string;
  associatedDeviceId?: string;
  associatedDeviceName?: string;
  associatedIncidentId?: string;
  associatedDeploymentId?: string;
  repository: string;
  branch: string;
  phase: AgentPhase;
  startTime: string;
  elapsedSeconds: number;
  tokenUsage?: {
    input: number;
    output: number;
    total: number;
  };
  lastEvent: string;
  lastEventTime: string;
  taskTree: TaskNode[];
  timeline: {
    id: string;
    type: 'instruction' | 'agent_summary' | 'file_change' | 'tool_request' | 'approval' | 'job_result' | 'verification';
    timestamp: string;
    title: string;
    description: string;
    metadata?: Record<string, any>;
  }[];
  requestedOperations: ToolRequest[];
  changedFiles: FileChangeDiff[];
  failureDetails?: {
    errorCode: string;
    message: string;
    remediationOptions: string[];
  };
}

export interface TelemetryPoint {
  timestamp: string;
  timeLabel: string;
  voltage: number;
  temperature: number;
  cpuUsage: number;
  heapFreeKB: number;
  packetLossPct: number;
  watchdogMarginMs: number;
  isMissing?: boolean;
}

export interface WatcherHealth {
  id: string;
  name: string;
  runtimeVersion: string;
  host: string;
  status: 'Healthy' | 'Degraded' | 'Unhealthy' | 'Disconnected';
  connectedDevicesCount: number;
  latencyMs: number;
  bufferDepthPct: number;
  dropRatePerSec: number;
  lastHeartbeat: string;
}

export interface MonitoringRule {
  id: string;
  name: string;
  scope: string; // e.g. "All ESP32-S3", "CM4-GATEWAY-BAY1"
  condition: string; // e.g. "voltage < 3.15V for 10s"
  windowSeconds: number;
  severity: IncidentSeverity;
  enabled: boolean;
  lastMatchTime?: string;
  matchCount24h: number;
  requiredSignals: string[];
  description: string;
}

export interface LogEntry {
  id: string;
  timestamp: string;
  level: 'DEBUG' | 'INFO' | 'WARN' | 'ERROR' | 'FATAL';
  sourceDevice: string;
  service: string;
  message: string;
  correlationIncidentId?: string;
  correlationJobId?: string;
}

export interface IncidentTimelineEvent {
  id: string;
  timestamp: string;
  type: 'detection' | 'evidence_collected' | 'agent_assigned' | 'operation_approved' | 'patch_deployed' | 'observation' | 'verification_passed' | 'resolved';
  title: string;
  actor: string;
  details: string;
}

export interface VerificationRequirement {
  id: string;
  description: string;
  signalSource: string;
  expectedCondition: string;
  currentObserved: string;
  passed: boolean;
  observationRemainingSec: number;
}

export interface Incident {
  id: string;
  title: string;
  severity: IncidentSeverity;
  status: IncidentStatus;
  affectedDeviceId: string;
  affectedDeviceName: string;
  affectedScope: string;
  gateway: string;
  owner: string;
  assignedAgent?: AgentProvider;
  occurrenceCount: number;
  openedAt: string;
  lastSeenAt: string;
  triggeringRule: string;
  evidenceSnapshot: {
    ruleName: string;
    rawSignalExcerpt: string;
    recentDiffSummary: string;
    firmwareVersionAtTrigger: string;
    connectivityState: string;
    boundedLogsExcerpt: string[];
  };
  investigation: {
    assignedAgent: AgentProvider;
    phase: string;
    diagnosisSummary: string;
    confidencePct?: number;
    proposedAction: string;
    changedFilesCount: number;
  };
  timeline: IncidentTimelineEvent[];
  verification: {
    requiredChecks: VerificationRequirement[];
    observationDurationSec: number;
    observationElapsedSec: number;
    allPassed: boolean;
  };
}

export type ArtifactType = 'Firmware (.bin/.elf)' | 'Device Config (.json)' | 'Edge AI Model (.tflite)' | 'App Container';
export type RolloutStrategy = 'All-at-Once' | 'Canary (10% Fleet)' | 'Rolling Batches (2x2)';

export interface PreflightCheck {
  id: string;
  name: string;
  category: string;
  status: 'passed' | 'failed' | 'running' | 'pending';
  measured: string;
  target: string;
}

export interface DeploymentDeviceResult {
  deviceId: string;
  deviceName: string;
  status: 'Queued' | 'Preflight' | 'Transferring' | 'Flashing' | 'Rebooting' | 'Verifying' | 'Completed' | 'Failed' | 'Rolled back';
  progressPct: number;
  currentStage: string;
  error?: string;
  firmwareApplied?: string;
}

export interface Deployment {
  id: string;
  releaseTag: string;
  artifactType: ArtifactType;
  artifactVersion: string;
  artifactDigestSha256: string;
  signatureStatus: 'Cryptographically Verified (Loki-Ed25519)' | 'Unsigned' | 'Invalid Signature';
  sourceRevision: string;
  targetFleetScope: string;
  targetDeviceIds: string[];
  initiator: {
    name: string;
    type: 'user' | 'agent' | 'policy';
    avatarLabel: string;
  };
  rolloutStrategy: RolloutStrategy;
  phase: 'queued' | 'preflight' | 'transferring' | 'flashing' | 'rebooting' | 'testing' | 'observing' | 'verified' | 'completed' | 'failed' | 'rolled_back';
  preflightChecks: PreflightCheck[];
  deviceResults: DeploymentDeviceResult[];
  observationWindowSec: number;
  observationElapsedSec: number;
  rollbackEligible: boolean;
  previousArtifactVersion?: string;
  startTime: string;
  completedTime?: string;
  associatedAgentRunId?: string;
  associatedIncidentId?: string;
  operationLogs: string[];
}

export interface ActivityEvent {
  id: string;
  timestamp: string;
  eventType:
    | 'DEVICE_CONNECT'
    | 'DEVICE_REBOOT'
    | 'APPROVAL_REQUESTED'
    | 'APPROVAL_GRANTED'
    | 'APPROVAL_DENIED'
    | 'AGENT_DISPATCH'
    | 'TOOL_EXECUTION'
    | 'FIRMWARE_FLASH'
    | 'INCIDENT_OPEN'
    | 'INCIDENT_RESOLVED'
    | 'DEPLOYMENT_TRIGGER'
    | 'DEPLOYMENT_VERIFIED'
    | 'CONFIG_CHANGE'
    | 'SECURITY_TOKEN_ROTATED';
  actor: {
    name: string;
    type: 'user' | 'agent' | 'gateway' | 'runtime' | 'policy';
    provider?: AgentProvider;
  };
  workspace: string;
  resourceType: 'Device' | 'Agent' | 'Incident' | 'Deployment' | 'Rule' | 'Runtime' | 'Security';
  resourceId: string;
  resourceName: string;
  actionSummary: string;
  summary?: string;
  result: 'Success' | 'Failed' | 'Approved' | 'Denied' | 'Warning' | 'Pending';
  riskLevel: RiskLevel;
  correlation?: {
    incidentId?: string;
    deploymentId?: string;
    agentRunId?: string;
    jobId?: string;
  };
  correlationId?: string;
  detailsPayload?: Record<string, any>;
  beforeAfterDiff?: {
    key: string;
    before: string;
    after: string;
  };
}

export interface ApprovalRequest {
  id: string;
  requestingActor: {
    name: string;
    type: 'agent' | 'user' | 'policy';
    provider?: AgentProvider;
  };
  operationName: string;
  targetDeviceIds: string[];
  targetDeviceNames: string[];
  inputs: Record<string, any>;
  reason: string;
  risk: RiskLevel;
  expectedEffectAndDowntime: string;
  preflightResult: {
    passed: boolean;
    summary: string;
    checks: { name: string; status: 'passed' | 'failed'; details: string }[];
  };
  verificationPlan: string;
  rollbackPlan: string;
  expiresAt: string;
  remainingMinutes: number;
  status: 'pending' | 'approved' | 'denied' | 'expired';
  associatedIncidentId?: string;
  associatedDeploymentId?: string;
  associatedAgentRunId?: string;
}

export interface Workspace {
  id: string;
  name: string;
  environment: 'production' | 'staging' | 'lab';
  region: string;
  connectedDevicesCount: number;
  runtimeVersion: string;
  runtimeConnected: boolean;
  gatewaysOnlineCount: number;
  gatewaysTotalCount: number;
}
