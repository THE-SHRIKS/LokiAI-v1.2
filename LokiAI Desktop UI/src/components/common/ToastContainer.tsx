import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { CheckCircle2, AlertCircle, Info, AlertTriangle, X, ExternalLink } from 'lucide-react';

export const ToastContainer: React.FC = () => {
  const { toasts, removeToast, navigateTo } = useLoki();

  if (toasts.length === 0) return null;

  return (
    <div
      id="toast-notifications-container"
      className="fixed bottom-4 right-4 z-50 flex flex-col gap-2 max-w-md w-full pointer-events-none"
      aria-live="polite"
    >
      {toasts.map((toast) => {
        const getIcon = () => {
          switch (toast.type) {
            case 'success':
              return <CheckCircle2 className="w-4 h-4 text-[#8AB55A] shrink-0" />;
            case 'error':
              return <AlertCircle className="w-4 h-4 text-[#CC272E] shrink-0" />;
            case 'warning':
              return <AlertTriangle className="w-4 h-4 text-[#E8D9A8] shrink-0" />;
            case 'info':
            default:
              return <Info className="w-4 h-4 text-[#8AB55A] shrink-0" />;
          }
        };

        return (
          <div
            key={toast.id}
            id={toast.id}
            className="pointer-events-auto flex items-start justify-between gap-3 p-3.5 rounded-[10px] bg-[#050505] border border-[#23221C] shadow-xl text-xs transition-all animate-in fade-in slide-in-from-bottom-2"
          >
            <div className="flex items-start gap-2.5">
              {getIcon()}
              <div className="space-y-0.5">
                <div className="font-semibold text-[#E8D9A8]">{toast.title}</div>
                <div className="text-[#8A8577] leading-relaxed">{toast.message}</div>
                {toast.jobId && (
                  <div className="font-mono text-[10px] text-[#8AB55A] pt-0.5">Job: {toast.jobId}</div>
                )}
                {toast.linkRoute && (
                  <button
                    onClick={() => navigateTo(toast.linkRoute!)}
                    className="inline-flex items-center gap-1 text-[11px] font-medium text-[#8AB55A] hover:underline pt-1"
                  >
                    View in {toast.linkRoute} <ExternalLink className="w-3 h-3" />
                  </button>
                )}
              </div>
            </div>

            <button
              onClick={() => removeToast(toast.id)}
              className="text-[#8A8577] hover:text-[#E8D9A8] p-1 rounded transition-colors"
              aria-label="Dismiss toast"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          </div>
        );
      })}
    </div>
  );
};
