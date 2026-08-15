import React, { useState } from 'react';
import { useLoki } from '../context/LokiContext';
import { AgentProvider } from '../types';
import { AgentTaskGraph } from '../components/common/AgentTaskGraph';
import { SeverityBadge } from '../components/common/SeverityBadge';
import { Bot, StopCircle, Play, FileCode, ShieldAlert, ArrowLeft } from 'lucide-react';

export const AgentsView: React.FC = () => {
  const {
    agentRuns,
    selectedAgentRunId,
    setSelectedAgentRunId,
    setActiveApprovalModal,
    approvals,
    openInspector,
    addToast,
  } = useLoki();

  const [providerFilter, setProviderFilter] = useState<string>('ALL');

  const filteredRuns = agentRuns.filter((run) => {
    if (providerFilter !== 'ALL' && run.provider !== providerFilter) return false;
    return true;
  });

  const selectedRun = agentRuns.find((r) => r.id === selectedAgentRunId);

  // Provider summary stats
  const providers: {
    name: AgentProvider;
    model: string;
    sessions: number;
    status: 'Online' | 'Active';
  }[] = [
    { name: 'Claude Code', model: 'claude-3-7-sonnet', sessions: 2, status: 'Active' },
    { name: 'OpenAI Codex', model: 'gpt-4o-hardware-specialist', sessions: 1, status: 'Active' },
    { name: 'Cursor CLI', model: 'cursor-agent-v2', sessions: 1, status: 'Online' },
    { name: 'Loki Agent Bridge', model: 'loki-hardware-ops-v1', sessions: 0, status: 'Online' },
  ];

  const handleStopAgent = (runId: string) => {
    addToast({
      title: 'Agent Paused',
      message: `Intervention signal sent to agent session ${runId}. State preserved.`,
      type: 'warning',
    });
  };

  const handleResumeAgent = (runId: string) => {
    addToast({
      title: 'Agent Resumed',
      message: `Agent session ${runId} resumed workflow execution.`,
      type: 'success',
    });
  };

  if (selectedRun) {
    const runApproval = approvals.find(
      (a) => a.associatedAgentRunId === selectedRun.id && a.status === 'pending',
    );
    const totalTokens = selectedRun.tokenUsage?.total || 0;

    return (
      <div id="agent-run-detail-view" className="space-y-6">
        {/* Detail Header */}
        <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setSelectedAgentRunId(null)}
              className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Back to Agent runs"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-base font-bold text-[#E8D9A8]">{selectedRun.taskTitle}</h2>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#23221C] text-[#8AB55A]">
                  {selectedRun.provider}
                </span>
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8A8577] uppercase">
                  Phase: {selectedRun.phase}
                </span>
              </div>
              <p className="text-xs text-[#8A8577] font-mono mt-0.5">
                Session ID: {selectedRun.id} • Repo:{' '}
                <span className="text-[#E8D9A8]">
                  {selectedRun.repository}#{selectedRun.branch}
                </span>{' '}
                • Elapsed: {Math.round(selectedRun.elapsedSeconds / 60)}m (
                {totalTokens.toLocaleString()} tokens)
              </p>
            </div>
          </div>

          <div className="flex items-center gap-2">
            {selectedRun.phase === 'waiting' && (
              <button
                onClick={() => handleResumeAgent(selectedRun.id)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#8AB55A] text-[#010101] text-xs font-bold hover:bg-[#A8C878] transition-colors"
              >
                <Play className="w-3.5 h-3.5" /> Resume Agent Run
              </button>
            )}

            {selectedRun.phase !== 'complete' && selectedRun.phase !== 'failed' && (
              <button
                onClick={() => handleStopAgent(selectedRun.id)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#23221C] text-[#CC272E] border border-[#CC272E]/40 hover:bg-[#CC272E]/10 text-xs font-mono transition-colors"
              >
                <StopCircle className="w-3.5 h-3.5" /> Intervene / Stop Agent
              </button>
            )}

            <button
              onClick={() => openInspector('agent', selectedRun)}
              className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#8AB55A] hover:bg-[#23221C] transition-colors"
            >
              Inspect Payload
            </button>
          </div>
        </div>

        {/* Pending Security Approval Banner if agent is blocked */}
        {runApproval && (
          <div className="p-4 rounded-[10px] bg-[#0A0A08] border border-[#8AB55A] flex flex-wrap items-center justify-between gap-4">
            <div className="flex items-start gap-3">
              <ShieldAlert className="w-5 h-5 text-[#8AB55A] shrink-0 mt-0.5" />
              <div>
                <div className="flex items-center gap-2">
                  <span className="text-xs font-bold text-[#E8D9A8]">
                    Agent Paused: Awaiting Security Authorization
                  </span>
                  <SeverityBadge severity={runApproval.risk} />
                </div>
                <p className="text-xs text-[#8A8577] mt-0.5">
                  The agent is requesting high-risk operation{' '}
                  <strong className="text-[#E8D9A8] font-mono">{runApproval.operationName}</strong>{' '}
                  on{' '}
                  <strong className="text-[#E8D9A8] font-mono">
                    {runApproval.targetDeviceNames.join(', ')}
                  </strong>
                  .
                </p>
              </div>
            </div>

            <button
              onClick={() => setActiveApprovalModal(runApproval)}
              className="px-4 py-1.5 rounded-[8px] bg-[#8AB55A] text-[#010101] text-xs font-bold hover:bg-[#A8C878] transition-colors"
            >
              Review Request ({runApproval.remainingMinutes}m remaining)
            </button>
          </div>
        )}

        {/* Multi-Agent Task Dependency Graph */}
        <AgentTaskGraph taskTree={selectedRun.taskTree} />

        {/* 2-Column: Changed Files & Chronological Timeline */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Changed Files */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 font-mono text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C] font-sans">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Modified Firmware & Driver Code
              </span>
              <span className="text-[11px] text-[#8A8577]">
                {selectedRun.changedFiles.length} files
              </span>
            </div>

            <div className="space-y-2">
              {selectedRun.changedFiles.map((file, idx) => (
                <div
                  key={idx}
                  className="p-2.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between"
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <FileCode className="w-4 h-4 text-[#8AB55A] shrink-0" />
                    <span className="text-[#E8D9A8] truncate">{file.path}</span>
                  </div>
                  <div className="flex items-center gap-2 text-[11px] shrink-0">
                    <span className="text-[#8AB55A]">+{file.additions}</span>
                    <span className="text-[#CC272E]">-{file.deletions}</span>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Chronological Event Timeline */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Task Execution Timeline
              </span>
              <span className="text-[11px] font-mono text-[#8A8577]">
                {selectedRun.timeline.length} events
              </span>
            </div>

            <div className="space-y-3">
              {selectedRun.timeline.map((event) => (
                <div
                  key={event.id}
                  className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] space-y-1"
                >
                  <div className="flex items-center justify-between font-mono text-[11px]">
                    <span className="font-semibold text-[#8AB55A] uppercase">{event.type}</span>
                    <span className="text-[#8A8577]">{event.timestamp}</span>
                  </div>
                  <p className="text-[#E8D9A8] leading-relaxed">{event.description}</p>
                </div>
              ))}
            </div>
          </div>
        </div>
      </div>
    );
  }

  // Agent Runs List View
  return (
    <div id="agents-view" className="space-y-6">
      {/* Top Connected Providers Summary */}
      <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
        {providers.map((p, idx) => (
          <div
            key={idx}
            className="p-3.5 rounded-[10px] bg-[#050505] border border-[#23221C] space-y-2 text-xs"
          >
            <div className="flex items-center justify-between">
              <span className="font-semibold text-[#E8D9A8] font-mono">{p.name}</span>
              <span className="w-2 h-2 rounded-full bg-[#8AB55A]"></span>
            </div>
            <div className="text-[11px] font-mono text-[#8A8577] truncate">{p.model}</div>
            <div className="flex justify-between text-[11px] font-mono text-[#8A8577] pt-1 border-t border-[#23221C]">
              <span>Active Sessions:</span>
              <span className="text-[#E8D9A8] font-bold">{p.sessions}</span>
            </div>
          </div>
        ))}
      </div>

      {/* Filter Bar */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex items-center gap-3">
          <Bot className="w-4 h-4 text-[#8AB55A]" />
          <span className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
            Agent Sessions & Automated Task Runs ({filteredRuns.length})
          </span>
        </div>

        <div className="flex items-center gap-2 font-mono text-xs">
          <select
            value={providerFilter}
            onChange={(e) => setProviderFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Providers</option>
            <option value="Claude Code">Claude Code</option>
            <option value="Codex / GPT-4o">Codex / GPT-4o</option>
            <option value="Cursor CLI Bridge">Cursor CLI Bridge</option>
          </select>
        </div>
      </div>

      {/* Runs Table */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="bg-[#090909] border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-3 px-4">Session & Task</th>
                <th className="py-3 px-4">Provider / Model</th>
                <th className="py-3 px-4">Execution Phase</th>
                <th className="py-3 px-4">Target Repo / Branch</th>
                <th className="py-3 px-4">Tokens Used</th>
                <th className="py-3 px-4">Duration</th>
                <th className="py-3 px-4 text-right">Inspect</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {filteredRuns.map((run) => (
                <tr
                  key={run.id}
                  id={`agent-run-row-${run.id}`}
                  onClick={() => setSelectedAgentRunId(run.id)}
                  className="hover:bg-[#0A0A08] cursor-pointer transition-colors group"
                >
                  <td className="py-3 px-4">
                    <div className="font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                      {run.taskTitle}
                    </div>
                    <div className="text-[10px] text-[#8A8577]">{run.id}</div>
                  </td>
                  <td className="py-3 px-4 font-semibold text-[#8AB55A]">{run.provider}</td>
                  <td className="py-3 px-4">
                    <span className="capitalize px-2 py-0.5 rounded bg-[#23221C] text-[#E8D9A8] text-[11px]">
                      {run.phase}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">
                    {run.repository}#{run.branch}
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">
                    {(run.tokenUsage?.total || 0).toLocaleString()}
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">
                    {Math.round(run.elapsedSeconds / 60)}m
                  </td>
                  <td className="py-3 px-4 text-right">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setSelectedAgentRunId(run.id);
                      }}
                      className="px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] text-[#8AB55A] hover:bg-[#23221C] transition-colors"
                    >
                      Inspect Tree
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
};
