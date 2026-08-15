import React from 'react';
import { GlobalStatus } from '../../types';
import {
  CheckCircle2,
  AlertTriangle,
  XCircle,
  Clock,
  Play,
  ShieldAlert,
  Search,
  RotateCcw,
  Ban,
  HelpCircle,
  Wifi,
  WifiOff,
} from 'lucide-react';

interface StatusChipProps {
  status: GlobalStatus;
  className?: string;
  showIcon?: boolean;
}

export const StatusChip: React.FC<StatusChipProps> = ({ status, className = '', showIcon = true }) => {
  const getStatusConfig = (s: GlobalStatus) => {
    switch (s) {
      case 'Healthy':
      case 'Connected':
      case 'Completed':
      case 'Resolved':
        return {
          icon: <CheckCircle2 className="w-3.5 h-3.5 text-[#8AB55A]" />,
          label: s,
          bg: 'bg-[#6B8C42]/10 border-[#6B8C42]/30 text-[#8AB55A]',
          aria: `${s} status`,
        };
      case 'Running':
      case 'Verifying':
        return {
          icon: (
            <span className="relative flex h-2 w-2 mr-1">
              <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#8AB55A] opacity-75"></span>
              <span className="relative inline-flex rounded-full h-2 w-2 bg-[#8AB55A]"></span>
            </span>
          ),
          label: s,
          bg: 'bg-[#6B8C42]/15 border-[#8AB55A]/40 text-[#E8D9A8]',
          aria: `${s} in progress`,
        };
      case 'Degraded':
      case 'Waiting':
        return {
          icon: <Clock className="w-3.5 h-3.5 text-[#E8D9A8]" />,
          label: s,
          bg: 'bg-[#23221C] border-[#8A8577]/40 text-[#E8D9A8]',
          aria: `${s} status`,
        };
      case 'Approval required':
        return {
          icon: <ShieldAlert className="w-3.5 h-3.5 text-[#E8D9A8]" />,
          label: s,
          bg: 'bg-[#23221C] border-[#8AB55A] text-[#E8D9A8] font-semibold',
          aria: 'Approval required for execution',
        };
      case 'Unhealthy':
      case 'Failed':
      case 'Disconnected':
        return {
          icon: <XCircle className="w-3.5 h-3.5 text-[#CC272E]" />,
          label: s,
          bg: 'bg-[#CC272E]/10 border-[#CC272E]/40 text-[#E8D9A8]',
          aria: `${s} failure state`,
        };
      case 'Rolled back':
      case 'Cancelled':
        return {
          icon: <RotateCcw className="w-3.5 h-3.5 text-[#8A8577]" />,
          label: s,
          bg: 'bg-[#0A0A08] border-[#23221C] text-[#8A8577]',
          aria: `${s} state`,
        };
      case 'Unknown':
      default:
        return {
          icon: <HelpCircle className="w-3.5 h-3.5 text-[#8A8577]" />,
          label: s,
          bg: 'bg-[#0A0A08] border-[#23221C] text-[#8A8577]',
          aria: 'Unknown status',
        };
    }
  };

  const config = getStatusConfig(status);

  return (
    <span
      id={`status-chip-${status.toLowerCase().replace(/\s+/g, '-')}`}
      className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-[8px] text-xs font-medium border tracking-wide whitespace-nowrap ${config.bg} ${className}`}
      role="status"
      aria-label={config.aria}
    >
      {showIcon && config.icon}
      <span>{config.label}</span>
    </span>
  );
};
