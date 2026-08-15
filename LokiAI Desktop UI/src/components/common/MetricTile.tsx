import React, { ReactNode } from 'react';

interface MetricTileProps {
  id: string;
  label: string;
  value: string | number;
  subValue?: string;
  icon?: ReactNode;
  statusColor?: 'olive' | 'neutral' | 'destructive' | 'warning';
  badge?: string;
  onClick?: () => void;
  className?: string;
}

export const MetricTile: React.FC<MetricTileProps> = ({
  id,
  label,
  value,
  subValue,
  icon,
  statusColor = 'neutral',
  badge,
  onClick,
  className = '',
}) => {
  const getBorderColor = () => {
    switch (statusColor) {
      case 'olive':
        return 'hover:border-[#8AB55A]/50 group-hover:border-[#8AB55A]/50';
      case 'destructive':
        return 'border-[#CC272E]/40 hover:border-[#CC272E]';
      case 'warning':
        return 'hover:border-[#E8D9A8]/40';
      default:
        return 'hover:border-[#8AB55A]/30';
    }
  };

  return (
    <div
      id={id}
      onClick={onClick}
      className={`relative bg-[#050505] border border-[#23221C] rounded-[10px] p-4 transition-all duration-150 group ${
        onClick ? 'cursor-pointer' : ''
      } ${getBorderColor()} ${className}`}
    >
      <div className="flex items-center justify-between gap-2 mb-2">
        <span className="text-xs font-medium text-[#8A8577] uppercase tracking-wider">{label}</span>
        <div className="flex items-center gap-1.5">
          {badge && (
            <span className="text-[10px] font-mono px-1.5 py-0.5 rounded-[4px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577]">
              {badge}
            </span>
          )}
          {icon && <div className="text-[#8A8577] group-hover:text-[#E8D9A8] transition-colors">{icon}</div>}
        </div>
      </div>

      <div className="flex items-baseline gap-2">
        <span className="text-2xl font-bold font-mono tracking-tight text-[#E8D9A8] tabular-nums">{value}</span>
        {subValue && <span className="text-xs font-mono text-[#8A8577] tabular-nums">{subValue}</span>}
      </div>
    </div>
  );
};
