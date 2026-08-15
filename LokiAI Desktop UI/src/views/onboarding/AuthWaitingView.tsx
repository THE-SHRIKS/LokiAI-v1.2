import React, { useState, useEffect } from 'react';
import { useLoki } from '../../context/LokiContext';
import {
  RefreshCw,
  CheckCircle2,
  AlertTriangle,
  Radio,
  ExternalLink,
  XCircle,
  Clock,
  ArrowRight,
  ShieldCheck,
  AlertCircle,
} from 'lucide-react';

export const AuthWaitingView: React.FC = () => {
  const { completeBrowserAuth, cancelBrowserAuth, addToast, userSession } = useLoki();

  const [state, setState] = useState<
    'waiting' | 'success' | 'expired' | 'network_error' | 'already_linked'
  >('waiting');
  const [secondsRemaining, setSecondsRemaining] = useState(120);
  const [callbackPort] = useState(18492);

  // Countdown timer for authentication expiration
  useEffect(() => {
    if (state !== 'waiting') return;
    const timer = setInterval(() => {
      setSecondsRemaining((prev) => {
        if (prev <= 1) {
          setState('expired');
          return 0;
        }
        return prev - 1;
      });
    }, 1000);
    return () => clearInterval(timer);
  }, [state]);

  const handleSimulateSuccess = () => {
    setState('success');
    addToast({
      type: 'success',
      title: 'Callback Received',
      message: 'Secure token verified by local daemon.',
    });

    setTimeout(() => {
      completeBrowserAuth();
    }, 1500);
  };

  const handleOpenBrowserAgain = () => {
    setSecondsRemaining(120);
    setState('waiting');
    addToast({
      type: 'info',
      title: 'Browser Auth Triggered',
      message: 'Re-opened https://auth.lokiai.io/authorize in default browser.',
    });
  };

  return (
    <div
      id="auth-waiting-screen"
      className="w-full max-w-xl mx-auto py-8 px-4 font-sans animate-in fade-in duration-200"
    >
      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Waiting State */}
        {state === 'waiting' && (
          <div className="space-y-6">
            <div className="text-center space-y-3">
              {/* Restrained animated olive pulse */}
              <div className="relative w-16 h-16 mx-auto flex items-center justify-center">
                <div className="absolute inset-0 rounded-full border border-[#8AB55A]/30 animate-ping"></div>
                <div className="w-12 h-12 rounded-full bg-[#050505] border border-[#8AB55A] flex items-center justify-center text-[#8AB55A]">
                  <RefreshCw className="w-5 h-5 animate-spin" style={{ animationDuration: '3s' }} />
                </div>
              </div>

              <div>
                <h1 className="text-lg font-bold text-[#E8D9A8]">
                  Finish signing in through your browser
                </h1>
                <p className="text-xs text-[#8A8577] mt-1">
                  Waiting for secure callback confirmation from <span className="text-[#E8D9A8]">auth.lokiai.io</span>
                </p>
              </div>
            </div>

            {/* Listener Diagnostics */}
            <div className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-2 text-xs font-mono">
              <div className="flex items-center justify-between">
                <span className="text-[#8A8577] flex items-center gap-1.5">
                  <Radio className="w-3.5 h-3.5 text-[#8AB55A]" /> Loopback Listener:
                </span>
                <span className="text-[#8AB55A]">http://127.0.0.1:{callbackPort}/callback</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[#8A8577] flex items-center gap-1.5">
                  <Clock className="w-3.5 h-3.5 text-[#8A8577]" /> Handshake Expiry:
                </span>
                <span className="text-[#E8D9A8]">{secondsRemaining}s remaining</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-[#8A8577]">State Token:</span>
                <span className="text-[#8A8577]">0x8f2a...99b2 (PKCE Active)</span>
              </div>
            </div>

            {/* Primary Action / Simulation Trigger */}
            <div className="space-y-3 pt-1">
              <button
                id="simulate-auth-success-btn"
                onClick={handleSimulateSuccess}
                className="w-full flex items-center justify-center gap-2 py-2.5 px-4 rounded-[8px] bg-[#8AB55A] hover:bg-[#A8C878] text-[#010101] font-bold text-xs transition-colors shadow"
              >
                <span>Simulate Browser Auth Callback (Success)</span>
                <ArrowRight className="w-3.5 h-3.5" />
              </button>

              <div className="flex items-center justify-between gap-3">
                <button
                  onClick={handleOpenBrowserAgain}
                  className="flex-1 flex items-center justify-center gap-1.5 py-2 px-3 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-medium transition-colors"
                >
                  <ExternalLink className="w-3.5 h-3.5 text-[#8A8577]" /> Open browser again
                </button>

                <button
                  onClick={cancelBrowserAuth}
                  className="flex-1 py-2 px-3 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#CC272E]/10 text-[#CC272E] text-xs font-medium transition-colors"
                >
                  Cancel
                </button>
              </div>
            </div>

            {/* Testing Edge Outcomes */}
            <div className="p-3 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-2">
              <div className="text-[10px] font-mono uppercase text-[#8A8577]">
                Prototype Outcome Simulators:
              </div>
              <div className="flex flex-wrap gap-2 text-xs">
                <button
                  onClick={() => setState('expired')}
                  className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8A8577] hover:text-[#E8D9A8] text-[11px] font-mono"
                >
                  Simulate Expiration
                </button>
                <button
                  onClick={() => setState('network_error')}
                  className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8A8577] hover:text-[#E8D9A8] text-[11px] font-mono"
                >
                  Simulate Network Error
                </button>
                <button
                  onClick={() => setState('already_linked')}
                  className="px-2 py-1 rounded bg-[#23221C] hover:bg-[#38362D] text-[#8A8577] hover:text-[#E8D9A8] text-[11px] font-mono"
                >
                  Simulate Already Linked
                </button>
              </div>
            </div>
          </div>
        )}

        {/* Success State */}
        {state === 'success' && (
          <div className="text-center space-y-5 py-4 animate-in fade-in zoom-in-95 duration-200">
            <div className="w-14 h-14 rounded-full bg-[#6B8C42]/20 border border-[#8AB55A] flex items-center justify-center text-[#8AB55A] mx-auto">
              <CheckCircle2 className="w-8 h-8" />
            </div>

            <div>
              <span className="text-[10px] font-mono uppercase tracking-widest text-[#8AB55A]">
                Handshake Verified
              </span>
              <h2 className="text-lg font-bold text-[#E8D9A8] mt-1">
                Account connected
              </h2>
              <p className="text-xs text-[#8A8577] mt-1">
                Connected to LokiAI Main Desktop Runtime
              </p>
            </div>

            <div className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] max-w-sm mx-auto text-left flex items-center gap-3.5">
              <div className="w-10 h-10 rounded-full bg-[#23221C] border border-[#8AB55A]/50 flex items-center justify-center text-sm font-mono font-bold text-[#E8D9A8]">
                LR
              </div>
              <div className="min-w-0 flex-1">
                <div className="text-xs font-bold text-[#E8D9A8]">Laukik Rajput</div>
                <div className="text-[11px] text-[#8A8577] font-mono">laukikxrajput@gmail.com</div>
                <div className="text-[10px] text-[#8AB55A] font-mono mt-0.5">Enterprise Fleet • Cluster Admin</div>
              </div>
            </div>

            <div className="text-[11px] text-[#8A8577] font-mono flex items-center justify-center gap-2">
              <RefreshCw className="w-3.5 h-3.5 animate-spin text-[#8AB55A]" />
              Continuing to workspace setup...
            </div>
          </div>
        )}

        {/* Expired State */}
        {state === 'expired' && (
          <div className="space-y-4 text-center py-2">
            <div className="w-12 h-12 rounded-full bg-[#CC272E]/10 border border-[#CC272E]/40 flex items-center justify-center text-[#CC272E] mx-auto">
              <Clock className="w-6 h-6" />
            </div>
            <div>
              <h2 className="text-base font-bold text-[#E8D9A8]">Authentication Timed Out</h2>
              <p className="text-xs text-[#8A8577] mt-1">
                The browser authorization request expired after 120 seconds for security reasons.
              </p>
            </div>
            <div className="flex gap-2 justify-center pt-2">
              <button
                onClick={handleOpenBrowserAgain}
                className="px-4 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] text-xs font-bold hover:bg-[#A8C878]"
              >
                Try Again
              </button>
              <button
                onClick={cancelBrowserAuth}
                className="px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] text-[#8A8577] text-xs hover:text-[#E8D9A8]"
              >
                Return to Sign In
              </button>
            </div>
          </div>
        )}

        {/* Network Error State */}
        {state === 'network_error' && (
          <div className="space-y-4 text-center py-2">
            <div className="w-12 h-12 rounded-full bg-[#CC272E]/10 border border-[#CC272E]/40 flex items-center justify-center text-[#CC272E] mx-auto">
              <AlertCircle className="w-6 h-6" />
            </div>
            <div>
              <h2 className="text-base font-bold text-[#E8D9A8]">Network Unavailable</h2>
              <p className="text-xs text-[#8A8577] mt-1">
                Unable to contact auth.lokiai.io. Check your network or firewall rules for port 18492.
              </p>
            </div>
            <div className="flex gap-2 justify-center pt-2">
              <button
                onClick={handleOpenBrowserAgain}
                className="px-4 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] text-xs font-bold hover:bg-[#A8C878]"
              >
                Retry Connection
              </button>
              <button
                onClick={cancelBrowserAuth}
                className="px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] text-[#8A8577] text-xs hover:text-[#E8D9A8]"
              >
                Cancel
              </button>
            </div>
          </div>
        )}

        {/* Already Linked State */}
        {state === 'already_linked' && (
          <div className="space-y-4 text-center py-2">
            <div className="w-12 h-12 rounded-full bg-[#8AB55A]/10 border border-[#8AB55A]/40 flex items-center justify-center text-[#8AB55A] mx-auto">
              <ShieldCheck className="w-6 h-6" />
            </div>
            <div>
              <h2 className="text-base font-bold text-[#E8D9A8]">Account Already Connected</h2>
              <p className="text-xs text-[#8A8577] mt-1">
                This runtime daemon is already paired with <strong className="text-[#E8D9A8]">laukikxrajput@gmail.com</strong>.
              </p>
            </div>
            <button
              onClick={handleSimulateSuccess}
              className="px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] text-xs font-bold hover:bg-[#A8C878]"
            >
              Continue with Existing Account
            </button>
          </div>
        )}
      </div>
    </div>
  );
};
