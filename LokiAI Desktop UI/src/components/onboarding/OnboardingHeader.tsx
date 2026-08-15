import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { ShieldCheck, LogOut, Radio, User, ChevronDown, RefreshCw } from 'lucide-react';

export const OnboardingHeader: React.FC = () => {
  const { authStatus, userSession, setIsLogoutModalOpen, activeAppRoute } = useLoki();
  const [isMenuOpen, setIsMenuOpen] = useState(false);

  return (
    <header
      id="onboarding-header"
      className="h-16 bg-[#050505] border-b border-[#23221C] px-6 flex items-center justify-between z-20 shrink-0 select-none"
    >
      {/* Top Left: Compact LokiAI Identity */}
      <div className="flex items-center gap-3">
        <div className="w-8 h-8 rounded-[8px] bg-[#0A0A08] border border-[#6B8C42]/50 flex items-center justify-center text-[#8AB55A] font-bold text-sm shadow-sm">
          λ
        </div>
        <div className="flex flex-col">
          <div className="flex items-center gap-2">
            <span className="text-sm font-bold tracking-tight text-[#E8D9A8] font-sans">
              LOKI<span className="text-[#8AB55A]">AI</span>
            </span>
            <span className="text-[10px] px-1.5 py-0.2 rounded bg-[#0A0A08] border border-[#23221C] font-mono uppercase text-[#8A8577]">
              Main Core
            </span>
          </div>
          <span className="text-[10px] font-mono text-[#8A8577]">
            Desktop Control Room • v1.2.0
          </span>
        </div>
      </div>

      {/* Top Right: Authentication State or Signed-in User */}
      <div className="flex items-center gap-3">
        {/* Local Daemon Status Pill */}
        <div className="hidden sm:flex items-center gap-2 px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] font-mono text-[#8A8577]">
          <span className="w-2 h-2 rounded-full bg-[#8AB55A] animate-pulse"></span>
          <span>Runtime: 127.0.0.1:3000 (Active)</span>
        </div>

        {authStatus === 'authenticated' && userSession ? (
          <div className="relative">
            <button
              id="onboarding-account-button"
              onClick={() => setIsMenuOpen(!isMenuOpen)}
              className="flex items-center gap-2.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 text-left transition-colors"
            >
              <div className="w-6 h-6 rounded-full bg-[#23221C] border border-[#8AB55A]/40 flex items-center justify-center text-[10px] font-mono font-bold text-[#E8D9A8]">
                {userSession.avatarInitials || 'LR'}
              </div>
              <div className="hidden md:block">
                <div className="text-xs font-semibold text-[#E8D9A8]">{userSession.name}</div>
                <div className="text-[10px] font-mono text-[#8A8577]">{userSession.email}</div>
              </div>
              <ChevronDown className="w-3.5 h-3.5 text-[#8A8577]" />
            </button>

            {isMenuOpen && (
              <div className="absolute top-full right-0 mt-2 w-64 bg-[#0F0F0C] border border-[#23221C] rounded-[10px] shadow-2xl p-1.5 z-50 text-xs space-y-1">
                <div className="px-2.5 py-2 border-b border-[#23221C]">
                  <div className="font-semibold text-[#E8D9A8]">{userSession.name}</div>
                  <div className="text-[10px] font-mono text-[#8A8577]">{userSession.email}</div>
                  <div className="text-[10px] text-[#8AB55A] mt-1 font-mono">{userSession.role}</div>
                </div>
                <div className="p-1">
                  <button
                    onClick={() => {
                      setIsMenuOpen(false);
                      setIsLogoutModalOpen(true);
                    }}
                    className="w-full flex items-center gap-2 px-2.5 py-1.5 rounded-[6px] text-[#CC272E] hover:bg-[#CC272E]/10 text-left transition-colors"
                  >
                    <LogOut className="w-3.5 h-3.5" /> Sign Out
                  </button>
                </div>
              </div>
            )}
          </div>
        ) : (
          <div className="flex items-center gap-1.5 text-xs text-[#8A8577] font-mono">
            <ShieldCheck className="w-4 h-4 text-[#8AB55A]" />
            <span>Secure Desktop Handshake</span>
          </div>
        )}
      </div>
    </header>
  );
};
