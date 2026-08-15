import React from 'react';
import { IncidentSeverity, RiskLevel } from '../../types';
import { AlertCircle, AlertOctagon, AlertTriangle, Info } from 'lucide-react';

interface SeverityBadgeProps {
  severity: IncidentSeverity | RiskLevel;
  className?: string;
}

export const SeverityBadge: React.FC<SeverityBadgeProps> = ({ severity, className = '' }) => {
  const getConfig = (sev: IncidentSeverity | RiskLevel) => {
    switch (sev) {
      case 'Critical':
        return {
          icon: <AlertOctagon className="w-3.5 h-3.5 text-[#CC272E]" />,
          bg: 'bg-[#CC272E]/15 border-[#CC272E]/50 text-[#E8D9A8]',
          label: 'Critical',
        };
      case 'High':
        return {
          icon: <AlertTriangle className="w-3.5 h-3.5 text-[#E8D9A8]" />,
          bg: 'bg-[#23221C] border-[#8AB55A]/50 text-[#E8D9A8]',
          label: 'High',
        };
      case 'Medium':
        return {
          icon: <AlertCircle className="w-3.5 h-3.5 text-[#8A8577]" />,
          bg: 'bg-[#0A0A08] border-[#23221C] text-[#E8D9A8]',
          label: 'Medium',
        };
      case 'Low':
      default:
        return {
          icon: <Info className="w-3.5 h-3.5 text-[#8A8577]" />,
          bg: 'bg-[#050505] border-[#23221C] text-[#8A8577]',
          label: 'Low',
        };
    }
  };

  const config = getConfig(severity);

  return (
    <span
      id={`severity-badge-${severity.toLowerCase()}`}
      className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-[8px] text-xs font-medium border tabular-nums ${config.bg} ${className}`}
    >
      {config.icon}
      <span>{config.label}</span>
    </span>
  );
};
