import React from 'react';
import { useLoki } from '../../context/LokiContext';
import { Activity, Radio } from 'lucide-react';

export const ConnectionIndicator: React.FC<{ compact?: boolean }> = ({ compact = false }) => {
  const { currentWorkspace, isLiveStreaming, setIsLiveStreaming } = useLoki();

  return (
    <div
      id="runtime-connection-indicator"
      className="flex items-center gap-2 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#8A8577]"
    >
      <div className="flex items-center gap-1.5">
        <span className="relative flex h-2 w-2">
          {currentWorkspace.runtimeConnected && isLiveStreaming && (
            <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#8AB55A] opacity-75"></span>
          )}
          <span
            className={`relative inline-flex rounded-full h-2 w-2 ${
              currentWorkspace.runtimeConnected ? 'bg-[#8AB55A]' : 'bg-[#CC272E]'
            }`}
          ></span>
        </span>
        <span className="font-mono text-[#E8D9A8] font-medium">
          {currentWorkspace.runtimeConnected ? 'Runtime Connected' : 'Runtime Offline'}
        </span>
      </div>

      {!compact && (
        <>
          <span className="text-[#23221C]">|</span>
          <div className="flex items-center gap-1 text-[#8A8577] tabular-nums">
            <Radio className="w-3 h-3 text-[#8AB55A]" />
            <span>
              {currentWorkspace.gatewaysOnlineCount}/{currentWorkspace.gatewaysTotalCount} Gateways
            </span>
          </div>
          <span className="text-[#23221C]">|</span>
          <button
            onClick={() => setIsLiveStreaming(!isLiveStreaming)}
            title={
              isLiveStreaming ? 'Click to pause visual streaming' : 'Click to resume live stream'
            }
            className="flex items-center gap-1 text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
          >
            <Activity
              className={`w-3 h-3 ${isLiveStreaming ? 'text-[#8AB55A]' : 'text-[#8A8577]'}`}
            />
            <span>{isLiveStreaming ? 'Live' : 'Paused'}</span>
          </button>
        </>
      )}
    </div>
  );
};
