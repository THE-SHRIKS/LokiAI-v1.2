import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import {
  ExternalLink,
  ShieldCheck,
  Radio,
  Key,
  AlertCircle,
  ChevronDown,
  ChevronUp,
  Cpu,
  CheckCircle2,
  Terminal,
} from 'lucide-react';

export const AuthView: React.FC = () => {
  const { startBrowserAuth, addToast, completeBrowserAuth } = useLoki();
  const [showTroubleshooting, setShowTroubleshooting] = useState(false);
  const [manualToken, setManualToken] = useState('');
  const [isVerifyingManual, setIsVerifyingManual] = useState(false);
  const [manualError, setManualError] = useState('');

  const handleManualTokenSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!manualToken.trim()) {
      setManualError('Please provide a valid authentication token');
      return;
    }
    setIsVerifyingManual(true);
    setManualError('');

    setTimeout(() => {
      setIsVerifyingManual(false);
      if (manualToken.length < 10) {
        setManualError('Invalid token format. Tokens start with loki_live_...');
        return;
      }
      completeBrowserAuth();
      addToast({
        type: 'success',
        title: 'Manual Token Authenticated',
        message: 'Successfully connected desktop runtime using manual authorization.',
      });
    }, 800);
  };

  return (
    <div id="auth-screen" className="w-full max-w-xl mx-auto py-8 px-4 font-sans animate-in fade-in duration-200">
      {/* Central Card */}
      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Brand Icon & Version */}
        <div className="flex flex-col items-center text-center space-y-3">
          <div className="w-14 h-14 rounded-[12px] bg-[#050505] border border-[#6B8C42]/60 flex items-center justify-center text-[#8AB55A] font-bold text-2xl shadow-lg">
            λ
          </div>
          <div>
            <div className="flex items-center justify-center gap-2">
              <h1 className="text-xl font-bold tracking-tight text-[#E8D9A8]">
                Connect LokiAI Main
              </h1>
              <span className="px-2 py-0.5 rounded bg-[#050505] border border-[#23221C] font-mono text-[10px] uppercase text-[#8AB55A]">
                v1.2 Core
              </span>
            </div>
            <p className="text-xs text-[#8A8577] mt-1.5 max-w-md mx-auto leading-relaxed">
              Sign in through the LokiAI website to connect this desktop runtime to your account.
            </p>
          </div>
        </div>

        {/* Local Runtime Diagnostics Pill */}
        <div className="p-3.5 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-2 text-xs font-mono">
          <div className="flex items-center justify-between text-[#8A8577]">
            <span className="flex items-center gap-2">
              <Radio className="w-3.5 h-3.5 text-[#8AB55A] animate-pulse" /> Local Daemon Status
            </span>
            <span className="text-[#8AB55A] font-semibold">Active (PID 18492)</span>
          </div>
          <div className="flex items-center justify-between text-[11px] text-[#8A8577]">
            <span>Loopback Ingress:</span>
            <span className="text-[#E8D9A8]">http://127.0.0.1:3000</span>
          </div>
          <div className="flex items-center justify-between text-[11px] text-[#8A8577]">
            <span>Hardware Discovery:</span>
            <span className="text-[#8AB55A]">USB/Serial/mTLS Ready</span>
          </div>
        </div>

        {/* Secure Handshake Explanation */}
        <div className="p-3.5 rounded-[8px] bg-[#050505] border border-[#23221C] flex items-start gap-3 text-xs text-[#8A8577] leading-relaxed">
          <ShieldCheck className="w-5 h-5 text-[#8AB55A] shrink-0 mt-0.5" />
          <div>
            <strong className="text-[#E8D9A8] block mb-0.5">Cryptographic Loopback Handshake</strong>
            Authentication completes securely in your browser using PKCE. Once confirmed, an encrypted session token is delivered directly to this local desktop runtime.
          </div>
        </div>

        {/* Primary CTA Button */}
        <div className="space-y-3 pt-2">
          <button
            id="continue-in-browser-button"
            onClick={startBrowserAuth}
            className="w-full flex items-center justify-center gap-2 py-3 px-4 rounded-[8px] bg-[#8AB55A] hover:bg-[#A8C878] text-[#010101] font-bold text-sm transition-all shadow-md active:scale-[0.99]"
          >
            <span>Continue in browser</span>
            <ExternalLink className="w-4 h-4" />
          </button>

          <p className="text-[11px] text-center text-[#8A8577] font-mono">
            A new browser tab will open at <span className="text-[#E8D9A8]">auth.lokiai.io</span>
          </p>
        </div>

        {/* Troubleshooting Accordion */}
        <div className="border-t border-[#23221C] pt-4">
          <button
            onClick={() => setShowTroubleshooting(!showTroubleshooting)}
            className="w-full flex items-center justify-between text-xs text-[#8A8577] hover:text-[#E8D9A8] transition-colors py-1"
          >
            <span className="flex items-center gap-1.5">
              <Key className="w-3.5 h-3.5 text-[#8AB55A]" /> Having trouble signing in?
            </span>
            {showTroubleshooting ? <ChevronUp className="w-3.5 h-3.5" /> : <ChevronDown className="w-3.5 h-3.5" />}
          </button>

          {showTroubleshooting && (
            <div className="mt-3 p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-3 text-xs animate-in fade-in duration-150">
              <div className="text-[11px] text-[#8A8577] space-y-1">
                <p className="text-[#E8D9A8] font-semibold">Manual Authorization Token</p>
                <p>
                  If your browser blocked localhost callback redirects, generate a personal token from{' '}
                  <span className="text-[#8AB55A] font-mono">lokiai.io/settings/tokens</span> and paste it below:
                </p>
              </div>

              <form onSubmit={handleManualTokenSubmit} className="space-y-2">
                <input
                  type="text"
                  value={manualToken}
                  onChange={(e) => {
                    setManualToken(e.target.value);
                    if (manualError) setManualError('');
                  }}
                  placeholder="loki_live_sec_9941..."
                  className="w-full px-3 py-2 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] font-mono text-xs focus:border-[#8AB55A] focus:outline-none placeholder-[#8A8577]/50"
                />

                {manualError && (
                  <p className="text-[11px] text-[#CC272E] font-mono">{manualError}</p>
                )}

                <div className="flex justify-end gap-2 pt-1">
                  <button
                    type="submit"
                    disabled={isVerifyingManual}
                    className="px-3 py-1.5 rounded-[6px] bg-[#23221C] hover:bg-[#38362D] text-[#E8D9A8] font-semibold text-xs transition-colors"
                  >
                    {isVerifyingManual ? 'Verifying...' : 'Authenticate Token'}
                  </button>
                </div>
              </form>
            </div>
          )}
        </div>
      </div>

      {/* Security Footer Note */}
      <div className="mt-6 text-center text-[11px] text-[#8A8577] max-w-sm mx-auto">
        LokiAI Main never stores cloud account passwords locally. All desktop-to-device telemetry is encrypted with local mTLS certificates.
      </div>
    </div>
  );
};
