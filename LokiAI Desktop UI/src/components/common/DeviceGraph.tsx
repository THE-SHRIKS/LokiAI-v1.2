import React, { useState } from 'react';
import { Device, DeviceRelationshipNode } from '../../types';
import { Network, Table, ZoomIn, ZoomOut, Maximize2, Cpu, Radio, Layers } from 'lucide-react';

interface DeviceGraphProps {
  device: Device;
  onSelectNode?: (node: DeviceRelationshipNode) => void;
}

export const DeviceGraph: React.FC<DeviceGraphProps> = ({ device, onSelectNode }) => {
  const [viewMode, setViewMode] = useState<'graph' | 'table'>('graph');
  const [zoomLevel, setZoomLevel] = useState(1);
  const [selectedNodeId, setSelectedNodeId] = useState<string>(device.id);

  const nodes = device.relationshipNodes || [];
  const edges = device.relationshipEdges || [];

  const handleNodeClick = (node: DeviceRelationshipNode) => {
    setSelectedNodeId(node.id);
    if (onSelectNode) onSelectNode(node);
  };

  const selectedNode = nodes.find((n) => n.id === selectedNodeId) || nodes[0];

  return (
    <div
      id="device-relationship-graph"
      className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4"
    >
      {/* Header Controls */}
      <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-[#23221C]">
        <div className="flex items-center gap-2">
          <Network className="w-4 h-4 text-[#8AB55A]" />
          <h4 className="text-xs font-semibold uppercase tracking-wider text-[#E8D9A8]">
            Topology & Relationship Hierarchy
          </h4>
        </div>

        <div className="flex items-center gap-2">
          <div className="flex items-center rounded-[6px] bg-[#0A0A08] border border-[#23221C] p-0.5">
            <button
              onClick={() => setZoomLevel((z) => Math.max(0.7, z - 0.1))}
              className="p-1 text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Zoom out"
            >
              <ZoomOut className="w-3.5 h-3.5" />
            </button>
            <span className="px-2 font-mono text-[11px] text-[#8A8577]">
              {Math.round(zoomLevel * 100)}%
            </span>
            <button
              onClick={() => setZoomLevel((z) => Math.min(1.4, z + 0.1))}
              className="p-1 text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Zoom in"
            >
              <ZoomIn className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => setZoomLevel(1)}
              className="p-1 text-[#8A8577] hover:text-[#E8D9A8] border-l border-[#23221C] transition-colors"
              title="Reset fit"
            >
              <Maximize2 className="w-3.5 h-3.5" />
            </button>
          </div>

          <button
            onClick={() => setViewMode(viewMode === 'graph' ? 'table' : 'graph')}
            className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            title={
              viewMode === 'graph' ? 'Switch to accessible table' : 'Switch to interactive graph'
            }
          >
            {viewMode === 'graph' ? (
              <Table className="w-3.5 h-3.5" />
            ) : (
              <Network className="w-3.5 h-3.5" />
            )}
          </button>
        </div>
      </div>

      {/* Main Container */}
      {viewMode === 'graph' ? (
        <div className="pt-3 grid grid-cols-1 lg:grid-cols-3 gap-4 items-start">
          {/* Interactive Visual Graph Canvas */}
          <div className="lg:col-span-2 relative min-h-[260px] bg-[#010101] border border-[#23221C] rounded-[8px] p-4 flex flex-col justify-center items-center overflow-hidden">
            <div
              className="flex flex-col gap-6 w-full max-w-md transition-transform duration-150"
              style={{ transform: `scale(${zoomLevel})` }}
            >
              {/* Nodes layout representation */}
              <div className="flex flex-col items-center gap-4">
                {/* Level 1: Gateway */}
                {nodes
                  .filter((n) => n.type === 'gateway')
                  .map((node) => (
                    <div
                      key={node.id}
                      onClick={() => handleNodeClick(node)}
                      className={`cursor-pointer px-4 py-2.5 rounded-[8px] border transition-all flex items-center gap-2.5 ${
                        selectedNodeId === node.id
                          ? 'bg-[#23221C] border-[#8AB55A] text-[#E8D9A8]'
                          : 'bg-[#050505] border-[#23221C] text-[#8A8577] hover:border-[#8A8577]'
                      }`}
                    >
                      <Radio className="w-4 h-4 text-[#8AB55A]" />
                      <div className="text-left">
                        <div className="text-xs font-semibold text-[#E8D9A8]">{node.name}</div>
                        <div className="text-[10px] text-[#8A8577] font-mono">{node.details}</div>
                      </div>
                    </div>
                  ))}

                {/* Connector Arrow */}
                <div className="flex flex-col items-center text-[#8A8577] text-[10px] font-mono">
                  <span>↓ {device.connector}</span>
                </div>

                {/* Level 2: Device Node (Primary) */}
                {nodes
                  .filter((n) => n.type === 'device')
                  .map((node) => (
                    <div
                      key={node.id}
                      onClick={() => handleNodeClick(node)}
                      className={`cursor-pointer px-5 py-3 rounded-[10px] border transition-all flex items-center gap-3 shadow-md ${
                        selectedNodeId === node.id
                          ? 'bg-[#0A0A08] border-[#8AB55A] ring-1 ring-[#8AB55A]'
                          : 'bg-[#050505] border-[#23221C] hover:border-[#8A8577]'
                      }`}
                    >
                      <Cpu className="w-5 h-5 text-[#8AB55A]" />
                      <div className="text-left">
                        <div className="text-xs font-bold text-[#E8D9A8]">
                          {node.name} (Primary Node)
                        </div>
                        <div className="text-[11px] text-[#8A8577] font-mono">{node.details}</div>
                      </div>
                    </div>
                  ))}

                {/* Connector Arrow */}
                <div className="flex flex-col items-center text-[#8A8577] text-[10px] font-mono">
                  <span>↓ Peripherals, Attached Sensors & Bus Lines</span>
                </div>

                {/* Level 3: Peripherals / Sensors / Bus / Project */}
                <div className="flex flex-wrap justify-center gap-3 w-full">
                  {nodes
                    .filter((n) => n.type !== 'gateway' && n.type !== 'device')
                    .map((node) => (
                      <div
                        key={node.id}
                        onClick={() => handleNodeClick(node)}
                        className={`cursor-pointer px-3 py-2 rounded-[8px] border transition-all flex items-center gap-2 text-xs ${
                          selectedNodeId === node.id
                            ? 'bg-[#23221C] border-[#8AB55A] text-[#E8D9A8]'
                            : 'bg-[#050505] border-[#23221C] text-[#8A8577] hover:border-[#8A8577]'
                        }`}
                      >
                        <Layers className="w-3.5 h-3.5 text-[#8AB55A]" />
                        <div>
                          <div className="font-medium text-[#E8D9A8]">{node.name}</div>
                          <div className="text-[10px] text-[#8A8577] font-mono">{node.details}</div>
                        </div>
                      </div>
                    ))}
                </div>
              </div>
            </div>
          </div>

          {/* Node Selection Inspector Panel */}
          {selectedNode && (
            <div className="bg-[#0A0A08] border border-[#23221C] rounded-[8px] p-3.5 space-y-3">
              <div className="flex items-center justify-between border-b border-[#23221C] pb-2">
                <span className="text-[11px] font-semibold text-[#8A8577] uppercase tracking-wider">
                  Node Inspector
                </span>
                <span className="text-[10px] font-mono px-1.5 py-0.5 rounded bg-[#23221C] text-[#8AB55A]">
                  {selectedNode.type}
                </span>
              </div>

              <div>
                <span className="text-xs font-bold text-[#E8D9A8] block">{selectedNode.name}</span>
                <span className="text-xs text-[#8A8577] font-mono">{selectedNode.details}</span>
              </div>

              <div className="p-2 rounded bg-[#050505] border border-[#23221C] text-xs space-y-1 font-mono">
                <div className="flex justify-between text-[#8A8577]">
                  <span>Status:</span>
                  <span className="text-[#8AB55A]">{selectedNode.status}</span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>ID:</span>
                  <span className="text-[#E8D9A8]">{selectedNode.id}</span>
                </div>
              </div>

              <div className="space-y-1.5">
                <span className="text-[11px] font-semibold text-[#8A8577] uppercase tracking-wider block">
                  Connected Edge Links
                </span>
                {edges
                  .filter((e) => e.source === selectedNode.id || e.target === selectedNode.id)
                  .map((e, idx) => (
                    <div
                      key={idx}
                      className="p-2 rounded bg-[#050505] border border-[#23221C] text-[11px] font-mono text-[#8A8577]"
                    >
                      <div className="text-[#E8D9A8] font-medium">{e.label}</div>
                      <div>Protocol: {e.protocol}</div>
                    </div>
                  ))}
              </div>
            </div>
          )}
        </div>
      ) : (
        /* Accessible Table Alternative */
        <div className="pt-3 overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-2 px-3">Node Name</th>
                <th className="py-2 px-3">Category Type</th>
                <th className="py-2 px-3">Status</th>
                <th className="py-2 px-3">Connection Details</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {nodes.map((n) => (
                <tr key={n.id} className="hover:bg-[#0A0A08]">
                  <td className="py-2 px-3 font-semibold">{n.name}</td>
                  <td className="py-2 px-3 uppercase text-[#8A8577]">{n.type}</td>
                  <td className="py-2 px-3 text-[#8AB55A]">{n.status}</td>
                  <td className="py-2 px-3 text-[#8A8577]">{n.details}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
