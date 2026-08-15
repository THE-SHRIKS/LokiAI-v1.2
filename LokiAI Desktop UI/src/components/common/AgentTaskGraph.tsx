import React, { useState } from 'react';
import { TaskNode } from '../../types';
import { GitBranch, CheckCircle2, Clock, XCircle, ArrowDown, Bot } from 'lucide-react';

interface AgentTaskGraphProps {
  taskTree: TaskNode[];
  onSelectTask?: (task: TaskNode) => void;
}

export const AgentTaskGraph: React.FC<AgentTaskGraphProps> = ({ taskTree, onSelectTask }) => {
  const [selectedTaskId, setSelectedTaskId] = useState<string>(taskTree[0]?.id || '');

  const handleSelect = (node: TaskNode) => {
    setSelectedTaskId(node.id);
    if (onSelectTask) onSelectTask(node);
  };

  const selectedNode = taskTree.find((t) => t.id === selectedTaskId) || taskTree[0];

  return (
    <div id="agent-task-graph" className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4">
      <div className="flex items-center justify-between pb-3 border-b border-[#23221C] mb-4">
        <div className="flex items-center gap-2">
          <GitBranch className="w-4 h-4 text-[#8AB55A]" />
          <h4 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
            Multi-Agent Task Dependency Graph
          </h4>
        </div>
        <span className="text-[11px] font-mono text-[#8A8577]">
          {taskTree.length} Orchestrated Tasks
        </span>
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-4 items-start">
        {/* Nodes flow layout */}
        <div className="lg:col-span-2 space-y-3">
          {taskTree.map((node, index) => {
            const isSelected = selectedTaskId === node.id;
            const getStatusIcon = () => {
              switch (node.status) {
                case 'completed':
                  return <CheckCircle2 className="w-4 h-4 text-[#8AB55A]" />;
                case 'running':
                  return (
                    <span className="relative flex h-2.5 w-2.5">
                      <span className="animate-ping absolute inline-flex h-full w-full rounded-full bg-[#8AB55A] opacity-75"></span>
                      <span className="relative inline-flex rounded-full h-2.5 w-2.5 bg-[#8AB55A]"></span>
                    </span>
                  );
                case 'failed':
                  return <XCircle className="w-4 h-4 text-[#CC272E]" />;
                case 'pending':
                default:
                  return <Clock className="w-4 h-4 text-[#8A8577]" />;
              }
            };

            return (
              <div key={node.id} className="relative">
                <div
                  id={`task-node-${node.id}`}
                  onClick={() => handleSelect(node)}
                  className={`cursor-pointer p-3 rounded-[8px] border transition-all flex items-start justify-between gap-3 ${
                    isSelected
                      ? 'bg-[#0A0A08] border-[#8AB55A] ring-1 ring-[#8AB55A]/50'
                      : 'bg-[#050505] border-[#23221C] hover:border-[#8A8577]'
                  }`}
                >
                  <div className="flex items-start gap-3">
                    <div className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] shrink-0 mt-0.5">
                      <Bot className="w-4 h-4 text-[#8AB55A]" />
                    </div>
                    <div>
                      <div className="flex items-center gap-2">
                        <span className="text-xs font-semibold text-[#E8D9A8]">{node.label}</span>
                        <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-[#23221C] text-[#8A8577]">
                          {node.agentRole}
                        </span>
                      </div>
                      {node.outputSummary && (
                        <p className="text-xs text-[#8A8577] mt-1 leading-relaxed line-clamp-2">
                          {node.outputSummary}
                        </p>
                      )}
                    </div>
                  </div>

                  <div className="flex flex-col items-end gap-1 shrink-0">
                    <div className="flex items-center gap-1.5">{getStatusIcon()}</div>
                    <span className="text-[10px] font-mono text-[#8A8577] tabular-nums">
                      {node.elapsedSec}s
                    </span>
                  </div>
                </div>

                {index < taskTree.length - 1 && (
                  <div className="flex justify-center my-1 text-[#23221C]">
                    <ArrowDown className="w-3.5 h-3.5 text-[#8A8577]" />
                  </div>
                )}
              </div>
            );
          })}
        </div>

        {/* Selected Task Inspection */}
        {selectedNode && (
          <div className="bg-[#0A0A08] border border-[#23221C] rounded-[8px] p-3.5 space-y-3 text-xs font-mono">
            <div className="flex items-center justify-between border-b border-[#23221C] pb-2 font-sans">
              <span className="text-[11px] font-semibold text-[#8A8577] uppercase tracking-wider">
                Task Inputs & Result
              </span>
              <span className="text-[10px] font-mono text-[#8AB55A] capitalize">
                {selectedNode.status}
              </span>
            </div>

            <div>
              <span className="text-xs font-bold text-[#E8D9A8] font-sans block">
                {selectedNode.label}
              </span>
              <span className="text-[11px] text-[#8A8577]">Role: {selectedNode.agentRole}</span>
            </div>

            <div>
              <span className="text-[10px] text-[#8A8577] uppercase block mb-1">Inputs:</span>
              <pre className="p-2 rounded bg-[#010101] border border-[#23221C] text-[11px] text-[#E8D9A8] overflow-x-auto whitespace-pre-wrap">
                {JSON.stringify(selectedNode.inputs, null, 2)}
              </pre>
            </div>

            {selectedNode.outputSummary && (
              <div>
                <span className="text-[10px] text-[#8A8577] uppercase block mb-1">
                  Execution Summary:
                </span>
                <div className="p-2 rounded bg-[#010101] border border-[#23221C] text-[11px] text-[#8AB55A] leading-relaxed">
                  {selectedNode.outputSummary}
                </div>
              </div>
            )}

            <div className="flex justify-between text-[#8A8577] pt-1 border-t border-[#23221C] text-[10px]">
              <span>Node ID: {selectedNode.id}</span>
              <span>Duration: {selectedNode.elapsedSec}s</span>
            </div>
          </div>
        )}
      </div>
    </div>
  );
};
