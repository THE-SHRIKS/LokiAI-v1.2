import React from 'react';
import { LokiProvider, useLoki } from './context/LokiContext';
import { AppShell } from './components/layout/AppShell';
import { OnboardingHeader } from './components/onboarding/OnboardingHeader';
import { ToastContainer } from './components/common/ToastContainer';
import { LogoutModal } from './components/common/LogoutModal';
import { AccountMenuModal } from './components/common/AccountMenuModal';

// Primary Dashboard Views
import { DashboardView } from './views/DashboardView';
import { DevicesView } from './views/DevicesView';
import { AgentsView } from './views/AgentsView';
import { MonitoringView } from './views/MonitoringView';
import { IncidentsView } from './views/IncidentsView';
import { DeploymentsView } from './views/DeploymentsView';
import { ActivityView } from './views/ActivityView';
import { SettingsView } from './views/SettingsView';

// Auth and Onboarding Views
import { AuthView } from './views/onboarding/AuthView';
import { AuthWaitingView } from './views/onboarding/AuthWaitingView';
import { WorkspaceSetupView } from './views/onboarding/WorkspaceSetupView';
import { AgentConnectView } from './views/onboarding/AgentConnectView';
import { DeviceChoiceView } from './views/onboarding/DeviceChoiceView';
import { DeviceConnectView } from './views/onboarding/DeviceConnectView';
import { DeviceHealthCheckView } from './views/onboarding/DeviceHealthCheckView';
import { ProjectSetupView } from './views/onboarding/ProjectSetupView';
import { ReadyReviewView } from './views/onboarding/ReadyReviewView';

const RouteSwitcher: React.FC = () => {
  const { currentRoute } = useLoki();

  switch (currentRoute) {
    case 'dashboard':
      return <DashboardView />;
    case 'devices':
      return <DevicesView />;
    case 'agents':
      return <AgentsView />;
    case 'monitoring':
      return <MonitoringView />;
    case 'incidents':
      return <IncidentsView />;
    case 'deployments':
      return <DeploymentsView />;
    case 'activity':
      return <ActivityView />;
    case 'settings':
      return <SettingsView />;
    default:
      return <DashboardView />;
  }
};

const AppContent: React.FC = () => {
  const { authStatus, activeAppRoute, onboardingState } = useLoki();

  // Guard: If not authenticated, always show Auth or AuthWaiting
  if (authStatus === 'unauthenticated') {
    return (
      <div className="min-h-screen w-full bg-[#010101] text-[#E8D9A8] flex flex-col antialiased">
        <OnboardingHeader />
        <main className="flex-1 flex items-center justify-center p-4">
          <AuthView />
        </main>
        <ToastContainer />
      </div>
    );
  }

  if (authStatus === 'waiting' || activeAppRoute === 'auth/waiting') {
    return (
      <div className="min-h-screen w-full bg-[#010101] text-[#E8D9A8] flex flex-col antialiased">
        <OnboardingHeader />
        <main className="flex-1 flex items-center justify-center p-4">
          <AuthWaitingView />
        </main>
        <ToastContainer />
      </div>
    );
  }

  // Onboarding Flows
  const isOnboardingRoute =
    activeAppRoute.startsWith('onboarding/') ||
    (!onboardingState.completed && activeAppRoute === 'dashboard');

  if (isOnboardingRoute) {
    const renderOnboardingStep = () => {
      switch (activeAppRoute) {
        case 'onboarding/workspace':
          return <WorkspaceSetupView />;
        case 'onboarding/agent':
          return <AgentConnectView />;
        case 'onboarding/device-choice':
          return <DeviceChoiceView />;
        case 'onboarding/device-connect':
          return <DeviceConnectView />;
        case 'onboarding/device-health':
          return <DeviceHealthCheckView />;
        case 'onboarding/project':
          return <ProjectSetupView />;
        case 'onboarding/ready':
          return <ReadyReviewView />;
        default:
          // Resume from saved step
          if (onboardingState.currentStep === 'agent') return <AgentConnectView />;
          if (onboardingState.currentStep === 'device') return <DeviceChoiceView />;
          if (onboardingState.currentStep === 'ready') return <ReadyReviewView />;
          return <WorkspaceSetupView />;
      }
    };

    return (
      <div className="min-h-screen w-full bg-[#010101] text-[#E8D9A8] flex flex-col antialiased">
        <OnboardingHeader />
        <main className="flex-1 flex items-start justify-center p-4 overflow-y-auto">
          {renderOnboardingStep()}
        </main>
        <ToastContainer />
        <LogoutModal />
        <AccountMenuModal />
      </div>
    );
  }

  // Protected Authenticated Workspace Dashboard Shell
  return (
    <AppShell>
      <RouteSwitcher />
    </AppShell>
  );
};

export default function App() {
  return (
    <LokiProvider>
      <AppContent />
    </LokiProvider>
  );
}
