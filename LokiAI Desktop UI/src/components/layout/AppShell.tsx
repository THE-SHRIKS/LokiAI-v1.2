import React, { ReactNode } from 'react';
import { Sidebar } from './Sidebar';
import { TopBar } from './TopBar';
import { InspectorDrawer } from '../common/InspectorDrawer';
import { ToastContainer } from '../common/ToastContainer';
import { CommandPalette } from '../common/CommandPalette';
import { ApprovalModal } from '../common/ApprovalModal';
import { NewDeploymentModal } from '../common/NewDeploymentModal';
import { DiscoverDeviceModal } from '../common/DiscoverDeviceModal';
import { LogoutModal } from '../common/LogoutModal';
import { AccountMenuModal } from '../common/AccountMenuModal';

export const AppShell: React.FC<{ children: ReactNode }> = ({ children }) => {
  return (
    <div className="flex h-screen w-screen overflow-hidden bg-[#010101] text-[#E8D9A8] antialiased font-sans select-none">
      {/* Sidebar */}
      <Sidebar />

      {/* Main Content Area */}
      <div className="flex flex-col flex-1 min-w-0 h-full overflow-hidden">
        <TopBar />
        <main className="flex-1 overflow-y-auto p-4 lg:p-6 bg-[#010101] select-text">
          <div className="max-w-7xl mx-auto w-full space-y-6">
            {children}
          </div>
        </main>
      </div>

      {/* Modals and Drawers */}
      <InspectorDrawer />
      <ToastContainer />
      <CommandPalette />
      <ApprovalModal />
      <NewDeploymentModal />
      <DiscoverDeviceModal />
      <LogoutModal />
      <AccountMenuModal />
    </div>
  );
};
