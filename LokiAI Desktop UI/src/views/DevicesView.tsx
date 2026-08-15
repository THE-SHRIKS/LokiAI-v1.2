import React, { useState, useMemo } from 'react';
import { useLoki } from '../context/LokiContext';
import { StatusChip } from '../components/common/StatusChip';
import { DeviceGraph } from '../components/common/DeviceGraph';
import { LogViewer } from '../components/common/LogViewer';
import { Cpu, Search, PlusCircle, RotateCcw, Play, ArrowLeft, Grid, List } from 'lucide-react';

export const DevicesView: React.FC = () => {
  const {
    devices,
    selectedDeviceId,
    setSelectedDeviceId,
    rebootDevice,
    triggerPreflight,
    triggerRollback,
    setIsAddDeviceModalOpen,
    setIsDeployModalOpen,
    openInspector,
    logs,
  } = useLoki();

  const [query, setQuery] = useState('');
  const [familyFilter, setFamilyFilter] = useState<string>('ALL');
  const [statusFilter, setStatusFilter] = useState<string>('ALL');
  const [viewMode, setViewMode] = useState<'table' | 'grid'>('table');

  const filteredDevices = useMemo(() => {
    return devices.filter((d) => {
      if (familyFilter !== 'ALL' && d.family !== familyFilter) return false;
      if (statusFilter !== 'ALL' && d.status !== statusFilter) return false;
      if (query.trim()) {
        const q = query.toLowerCase();
        return (
          d.name.toLowerCase().includes(q) ||
          d.hardwareId.toLowerCase().includes(q) ||
          d.family.toLowerCase().includes(q) ||
          d.transportPort.toLowerCase().includes(q) ||
          d.firmwareVersion.toLowerCase().includes(q)
        );
      }
      return true;
    });
  }, [devices, familyFilter, statusFilter, query]);

  const selectedDevice = devices.find((d) => d.id === selectedDeviceId);

  // Device Detail Sub-View
  if (selectedDevice) {
    const deviceLogs = logs.filter((l) => l.sourceDevice === selectedDevice.name);

    return (
      <div id="device-detail-view" className="space-y-6">
        {/* Detail Header */}
        <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
          <div className="flex items-center gap-3">
            <button
              onClick={() => setSelectedDeviceId(null)}
              className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
              title="Back to device inventory"
            >
              <ArrowLeft className="w-4 h-4" />
            </button>
            <div>
              <div className="flex items-center gap-2">
                <h2 className="text-base font-bold text-[#E8D9A8] font-mono">
                  {selectedDevice.name}
                </h2>
                <StatusChip status={selectedDevice.status} />
                <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A]">
                  {selectedDevice.supportTier}
                </span>
              </div>
              <p className="text-xs text-[#8A8577] font-mono mt-0.5 break-all">
                Hardware UID: {selectedDevice.hardwareId} • Last seen: {selectedDevice.lastSeen}
              </p>
            </div>
          </div>

          {/* Safe Contextual Actions */}
          <div className="flex flex-wrap items-center gap-2">
            <button
              onClick={() => triggerPreflight(selectedDevice.id)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] hover:border-[#8AB55A]/50 transition-colors"
            >
              <Play className="w-3.5 h-3.5 text-[#8AB55A]" /> Run Preflight
            </button>

            <button
              onClick={() => rebootDevice(selectedDevice.id)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] hover:border-[#8AB55A]/50 transition-colors"
            >
              <RotateCcw className="w-3.5 h-3.5 text-[#8A8577]" /> Safe Reboot
            </button>

            <button
              onClick={() => setIsDeployModalOpen(true)}
              className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors"
            >
              Flash Firmware
            </button>
          </div>
        </div>

        {/* 2-Column Overview & Health Metrics */}
        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Hardware Identity & Transport */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3 font-mono text-xs">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C] font-sans">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Hardware & Transport Specification
              </span>
              <button
                onClick={() => openInspector('device', selectedDevice)}
                className="text-[11px] text-[#8AB55A] hover:underline"
              >
                Inspect Data Contract
              </button>
            </div>

            <div className="grid grid-cols-2 gap-3 text-xs">
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Family & Architecture
                </span>
                <span className="text-[#E8D9A8] font-bold">{selectedDevice.family}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Manufacturer & Board
                </span>
                <span className="text-[#E8D9A8]">
                  {selectedDevice.manufacturer} ({selectedDevice.boardRevision})
                </span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Physical Connector
                </span>
                <span className="text-[#8AB55A]">{selectedDevice.connector}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Transport Port / URI
                </span>
                <span className="text-[#E8D9A8]">{selectedDevice.transportPort}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Active Firmware
                </span>
                <span className="text-[#8AB55A] font-bold">{selectedDevice.firmwareVersion}</span>
              </div>
              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C]">
                <span className="text-[10px] text-[#8A8577] uppercase block mb-0.5">
                  Rollback Target
                </span>
                <span className="text-[#E8D9A8]">
                  {selectedDevice.previousFirmwareVersion || 'None'}
                </span>
              </div>
            </div>

            {/* Advertised Capabilities */}
            <div>
              <span className="text-[11px] font-sans font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
                Advertised Hardware Capabilities
              </span>
              <div className="flex flex-wrap gap-1.5">
                {selectedDevice.capabilities.map((cap) => (
                  <span
                    key={cap}
                    className="px-2 py-0.5 rounded-[4px] bg-[#0A0A08] border border-[#23221C] text-[11px] font-mono text-[#8AB55A]"
                  >
                    +{cap}
                  </span>
                ))}
              </div>
            </div>
          </div>

          {/* Health Checks & Sensors */}
          <div className="bg-[#050505] border border-[#23221C] rounded-[10px] p-4 space-y-3">
            <div className="flex items-center justify-between pb-2 border-b border-[#23221C]">
              <span className="text-xs font-semibold uppercase tracking-wider text-[#8A8577]">
                Continuous Health Check Assertions
              </span>
              <span className="text-[11px] font-mono text-[#8AB55A]">
                {selectedDevice.healthChecks.filter((h) => h.status === 'Healthy').length}/
                {selectedDevice.healthChecks.length} Passed
              </span>
            </div>

            <div className="space-y-2">
              {selectedDevice.healthChecks.map((chk) => (
                <div
                  key={chk.id}
                  className="p-2.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between text-xs"
                >
                  <div>
                    <div className="flex items-center gap-2">
                      <span className="font-semibold text-[#E8D9A8]">{chk.name}</span>
                      <span className="text-[10px] font-mono px-1.5 py-0.2 rounded bg-[#23221C] text-[#8A8577]">
                        {chk.category}
                      </span>
                    </div>
                    {chk.details && (
                      <p className="text-[11px] text-[#CC272E] mt-0.5 font-mono">{chk.details}</p>
                    )}
                  </div>

                  <div className="flex flex-col items-end font-mono">
                    <span
                      className={`font-bold ${chk.status === 'Healthy' ? 'text-[#8AB55A]' : 'text-[#CC272E]'}`}
                    >
                      {chk.measuredValue}
                    </span>
                    <span className="text-[10px] text-[#8A8577]">Target: {chk.threshold}</span>
                  </div>
                </div>
              ))}
            </div>

            {selectedDevice.rollbackEligible && (
              <div className="p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C] flex items-center justify-between font-mono text-xs">
                <div>
                  <span className="text-[#E8D9A8] font-semibold block">
                    Rollback Safeguard Active
                  </span>
                  <span className="text-[#8A8577] text-[11px]">
                    Previous validated: {selectedDevice.previousFirmwareVersion}
                  </span>
                </div>
                <button
                  onClick={() => triggerRollback(selectedDevice.id)}
                  className="px-3 py-1 rounded-[6px] bg-[#23221C] hover:bg-[#38362D] text-[#E8D9A8] text-xs transition-colors"
                >
                  Trigger Rollback
                </button>
              </div>
            )}
          </div>
        </div>

        {/* Interactive Device Topology Graph */}
        <DeviceGraph device={selectedDevice} />

        {/* Device-Scoped Logs */}
        <LogViewer logs={deviceLogs} title={`Operational Logs for ${selectedDevice.name}`} />
      </div>
    );
  }

  // Device List / Inventory View
  return (
    <div id="devices-view" className="space-y-6">
      {/* Top Filter Bar */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex flex-wrap items-center gap-3">
          {/* Search Box */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-[#8A8577] absolute left-3 top-2.5" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search by name, UID, family, or port..."
              className="pl-8 pr-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] placeholder-[#8A8577] focus:outline-none focus:border-[#8AB55A] w-52 sm:w-64 font-mono"
            />
          </div>

          {/* Family Filter */}
          <select
            value={familyFilter}
            onChange={(e) => setFamilyFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Families (6)</option>
            <option value="ESP32-S3">ESP32-S3</option>
            <option value="STM32-H7">STM32-H7</option>
            <option value="Raspberry Pi CM4">Raspberry Pi CM4</option>
            <option value="nRF5340">nRF5340</option>
            <option value="RP2040-CAN">RP2040-CAN</option>
            <option value="i.MX8M-Plus">i.MX8M-Plus</option>
          </select>

          {/* Status Filter */}
          <select
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Statuses</option>
            <option value="Healthy">Healthy</option>
            <option value="Degraded">Degraded</option>
            <option value="Unhealthy">Unhealthy</option>
          </select>
        </div>

        <div className="flex items-center gap-2">
          {/* Table / Grid Toggle */}
          <div className="flex items-center rounded-[6px] bg-[#0A0A08] border border-[#23221C] p-0.5">
            <button
              onClick={() => setViewMode('table')}
              className={`p-1.5 rounded-[4px] transition-colors ${
                viewMode === 'table'
                  ? 'bg-[#23221C] text-[#8AB55A]'
                  : 'text-[#8A8577] hover:text-[#E8D9A8]'
              }`}
              title="Table view"
            >
              <List className="w-3.5 h-3.5" />
            </button>
            <button
              onClick={() => setViewMode('grid')}
              className={`p-1.5 rounded-[4px] transition-colors ${
                viewMode === 'grid'
                  ? 'bg-[#23221C] text-[#8AB55A]'
                  : 'text-[#8A8577] hover:text-[#E8D9A8]'
              }`}
              title="Grid view"
            >
              <Grid className="w-3.5 h-3.5" />
            </button>
          </div>

          <button
            onClick={() => setIsAddDeviceModalOpen(true)}
            className="flex items-center gap-1.5 px-3.5 py-1.5 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors shadow-sm"
          >
            <PlusCircle className="w-3.5 h-3.5" /> Discover / Add Device
          </button>
        </div>
      </div>

      {/* Main Device Inventory Presentation */}
      {viewMode === 'table' ? (
        <div className="bg-[#050505] border border-[#23221C] rounded-[10px] overflow-hidden">
          <div className="overflow-x-auto">
            <table className="w-full text-left text-xs font-mono">
              <thead className="bg-[#090909] border-b border-[#23221C] text-[#8A8577] text-[11px]">
                <tr>
                  <th className="py-3 px-4">Device Identity</th>
                  <th className="py-3 px-4">Family / Arch</th>
                  <th className="py-3 px-4">Transport Connector</th>
                  <th className="py-3 px-4">Operational Status</th>
                  <th className="py-3 px-4">Active Firmware</th>
                  <th className="py-3 px-4">Gateway</th>
                  <th className="py-3 px-4">Heartbeat</th>
                  <th className="py-3 px-4 text-right">Actions</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
                {filteredDevices.map((dev) => (
                  <tr
                    key={dev.id}
                    id={`device-row-${dev.id}`}
                    onClick={() => setSelectedDeviceId(dev.id)}
                    className="hover:bg-[#0A0A08] cursor-pointer transition-colors group"
                  >
                    <td className="py-3 px-4">
                      <div className="font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                        {dev.name}
                      </div>
                      <div className="text-[10px] text-[#8A8577] truncate max-w-xs">
                        {dev.hardwareId}
                      </div>
                    </td>
                    <td className="py-3 px-4 font-medium">{dev.family}</td>
                    <td className="py-3 px-4 text-[#8A8577]">{dev.connector}</td>
                    <td className="py-3 px-4">
                      <StatusChip status={dev.status} />
                    </td>
                    <td className="py-3 px-4 text-[#8AB55A] font-semibold">
                      {dev.firmwareVersion}
                    </td>
                    <td className="py-3 px-4 text-[#8A8577]">{dev.gatewayName.split(' ')[0]}</td>
                    <td className="py-3 px-4 text-[#8A8577]">{dev.lastSeen}</td>
                    <td className="py-3 px-4 text-right">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          setSelectedDeviceId(dev.id);
                        }}
                        className="px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] text-[#8AB55A] hover:bg-[#23221C] transition-colors"
                      >
                        Inspect
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      ) : (
        /* Compact Grid View */
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {filteredDevices.map((dev) => (
            <div
              key={dev.id}
              onClick={() => setSelectedDeviceId(dev.id)}
              className="p-4 rounded-[10px] bg-[#050505] border border-[#23221C] hover:border-[#8AB55A]/40 transition-all cursor-pointer space-y-3 group"
            >
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <Cpu className="w-4 h-4 text-[#8AB55A]" />
                  <span className="font-bold text-xs font-mono text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                    {dev.name}
                  </span>
                </div>
                <StatusChip status={dev.status} />
              </div>

              <div className="p-2.5 rounded bg-[#0A0A08] border border-[#23221C] text-xs font-mono space-y-1">
                <div className="flex justify-between text-[#8A8577]">
                  <span>Family:</span> <span className="text-[#E8D9A8]">{dev.family}</span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>Firmware:</span>{' '}
                  <span className="text-[#8AB55A]">{dev.firmwareVersion}</span>
                </div>
                <div className="flex justify-between text-[#8A8577]">
                  <span>Port:</span>{' '}
                  <span className="text-[#E8D9A8] truncate max-w-[150px]">{dev.transportPort}</span>
                </div>
              </div>

              <div className="flex items-center justify-between text-[11px] font-mono text-[#8A8577] pt-1 border-t border-[#23221C]">
                <span>Last seen: {dev.lastSeen}</span>
                <span className="text-[#8AB55A]">Inspect →</span>
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
};
