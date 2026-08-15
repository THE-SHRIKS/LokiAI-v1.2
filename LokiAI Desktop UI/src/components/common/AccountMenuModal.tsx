import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { Key, ExternalLink, X, LogOut, Building } from 'lucide-react';

interface AccountMenuModalProps {
  // Optional, not wired by any call site yet (App.tsx, AppShell.tsx both mount
  // this with no props) — this component has always rendered null in the
  // shipped baseline. FND-002 only makes that existing, pre-existing gap type-
  // check honestly; wiring real open/close state is separate UI-node work, not
  // toolchain/workspace foundation.
  isOpen?: boolean;
  onClose?: () => void;
  initialTab?: 'profile' | 'organization' | 'plan' | 'runtime';
}

export const AccountMenuModal: React.FC<AccountMenuModalProps> = ({
  isOpen = false,
  onClose = () => {},
  initialTab = 'profile',
}) => {
  const { userSession, setIsLogoutModalOpen } = useLoki();
  const [activeTab, setActiveTab] = React.useState(initialTab);

  if (!isOpen || !userSession) return null;

  return (
    <div
      id="account-details-modal"
      className="fixed inset-0 bg-[#010101]/85 backdrop-blur-sm z-50 flex items-center justify-center p-4"
      role="dialog"
      aria-modal="true"
    >
      <div className="w-full max-w-2xl bg-[#0A0A08] border border-[#23221C] rounded-[12px] shadow-2xl overflow-hidden font-sans flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="px-6 py-4 bg-[#0F0F0C] border-b border-[#23221C] flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-full bg-[#23221C] border border-[#8AB55A]/40 flex items-center justify-center text-xs font-mono font-bold text-[#E8D9A8]">
              {userSession.avatarInitials || 'LR'}
            </div>
            <div>
              <h2 className="text-sm font-bold text-[#E8D9A8]">{userSession.name}</h2>
              <span className="text-[11px] font-mono text-[#8A8577]">{userSession.email}</span>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-1 rounded-[6px] text-[#8A8577] hover:text-[#E8D9A8] hover:bg-[#23221C]"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Tab Bar */}
        <div className="px-6 border-b border-[#23221C] bg-[#050505] flex gap-4 text-xs font-semibold">
          <button
            onClick={() => setActiveTab('profile')}
            className={`py-3 border-b-2 transition-colors ${
              activeTab === 'profile'
                ? 'border-[#8AB55A] text-[#E8D9A8]'
                : 'border-transparent text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Profile & Security
          </button>
          <button
            onClick={() => setActiveTab('organization')}
            className={`py-3 border-b-2 transition-colors ${
              activeTab === 'organization'
                ? 'border-[#8AB55A] text-[#E8D9A8]'
                : 'border-transparent text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Organization & Team
          </button>
          <button
            onClick={() => setActiveTab('plan')}
            className={`py-3 border-b-2 transition-colors ${
              activeTab === 'plan'
                ? 'border-[#8AB55A] text-[#E8D9A8]'
                : 'border-transparent text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Plan & Fleet Billing
          </button>
          <button
            onClick={() => setActiveTab('runtime')}
            className={`py-3 border-b-2 transition-colors ${
              activeTab === 'runtime'
                ? 'border-[#8AB55A] text-[#E8D9A8]'
                : 'border-transparent text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Runtime Downloads
          </button>
        </div>

        {/* Tab Content */}
        <div className="p-6 overflow-y-auto space-y-4 text-xs">
          {activeTab === 'profile' && (
            <div className="space-y-4">
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
                <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
                  <span className="text-[10px] text-[#8A8577] uppercase block mb-1 font-mono">
                    User ID
                  </span>
                  <span className="font-mono text-[#E8D9A8]">{userSession.id}</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
                  <span className="text-[10px] text-[#8A8577] uppercase block mb-1 font-mono">
                    Role
                  </span>
                  <span className="font-semibold text-[#8AB55A]">{userSession.role}</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
                  <span className="text-[10px] text-[#8A8577] uppercase block mb-1 font-mono">
                    Token Expiration
                  </span>
                  <span className="font-mono text-[#E8D9A8]">{userSession.tokenExpiresAt}</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C]">
                  <span className="text-[10px] text-[#8A8577] uppercase block mb-1 font-mono">
                    Auth Handshake
                  </span>
                  <span className="font-mono text-[#8AB55A]">PKCE Verified (mTLS)</span>
                </div>
              </div>

              <div className="p-3.5 rounded bg-[#050505] border border-[#23221C] space-y-2">
                <div className="flex items-center justify-between">
                  <div className="flex items-center gap-2 font-semibold text-[#E8D9A8]">
                    <Key className="w-4 h-4 text-[#8AB55A]" /> Hardware Operator SSH Keys
                  </div>
                  <span className="text-[10px] font-mono text-[#8AB55A]">2 Active Keys</span>
                </div>
                <p className="text-[#8A8577] text-[11px]">
                  Keys loaded by Loki daemon for automated SWD flash and secure serial bridge
                  verification.
                </p>
              </div>
            </div>
          )}

          {activeTab === 'organization' && (
            <div className="space-y-4">
              <div className="p-4 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <div className="flex items-center gap-3">
                  <div className="w-9 h-9 rounded bg-[#23221C] flex items-center justify-center text-[#8AB55A]">
                    <Building className="w-5 h-5" />
                  </div>
                  <div>
                    <div className="font-bold text-[#E8D9A8] text-sm">
                      {userSession.organization}
                    </div>
                    <div className="text-[11px] text-[#8A8577] font-mono">
                      Team: Hardware & Reliability Engineering
                    </div>
                  </div>
                </div>
                <span className="px-2 py-0.5 rounded bg-[#6B8C42]/20 border border-[#8AB55A]/40 text-[#8AB55A] font-mono text-[10px]">
                  ENTERPRISE
                </span>
              </div>

              <div className="p-3.5 rounded bg-[#050505] border border-[#23221C] space-y-2">
                <div className="font-semibold text-[#E8D9A8]">Cluster Permissions</div>
                <p className="text-[#8A8577] text-[11px]">
                  All high-risk physical hardware operations (voltage override, flash overwrite) are
                  logged to the immutable audit trail.
                </p>
              </div>
            </div>
          )}

          {activeTab === 'plan' && (
            <div className="space-y-4">
              <div className="p-4 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                <div>
                  <span className="text-[10px] uppercase font-mono text-[#8AB55A] font-bold">
                    Active License
                  </span>
                  <h3 className="text-base font-bold text-[#E8D9A8]">{userSession.plan}</h3>
                  <p className="text-[11px] text-[#8A8577] mt-0.5">
                    Unlimited local daemons • 256 connected nodes • Multi-agent orchestration
                  </p>
                </div>
                <a
                  href="https://lokiai.io/billing"
                  target="_blank"
                  rel="noreferrer"
                  className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#8AB55A] text-[#010101] font-bold hover:bg-[#A8C878] transition-colors text-xs"
                >
                  Manage Capacity <ExternalLink className="w-3 h-3" />
                </a>
              </div>
            </div>
          )}

          {activeTab === 'runtime' && (
            <div className="space-y-3">
              <p className="text-[#8A8577] text-[11px]">
                Install local daemon binaries on other test bench machines, Raspberry Pis, or CI/CD
                hardware runners:
              </p>
              <div className="grid grid-cols-1 sm:grid-cols-2 gap-2 font-mono text-[11px]">
                <div className="p-3 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                  <span>Linux (x86_64 / arm64)</span>
                  <span className="text-[#8AB55A]">v1.2.0 (.deb / .tar.gz)</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                  <span>macOS (Apple Silicon / Intel)</span>
                  <span className="text-[#8AB55A]">v1.2.0 (.pkg)</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                  <span>Windows (x64)</span>
                  <span className="text-[#8AB55A]">v1.2.0 (.msi)</span>
                </div>
                <div className="p-3 rounded bg-[#050505] border border-[#23221C] flex items-center justify-between">
                  <span>Embedded Daemon (CM4 / Yocto)</span>
                  <span className="text-[#8AB55A]">v1.2.0 (ipk)</span>
                </div>
              </div>
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="px-6 py-3.5 bg-[#0F0F0C] border-t border-[#23221C] flex items-center justify-between">
          <button
            onClick={() => {
              onClose();
              setIsLogoutModalOpen(true);
            }}
            className="flex items-center gap-1.5 text-xs text-[#CC272E] hover:underline font-semibold"
          >
            <LogOut className="w-3.5 h-3.5" /> Sign Out of Desktop
          </button>
          <button
            onClick={onClose}
            className="px-4 py-1.5 rounded-[8px] bg-[#23221C] text-[#E8D9A8] hover:bg-[#38362D] text-xs font-semibold"
          >
            Close
          </button>
        </div>
      </div>
    </div>
  );
};
