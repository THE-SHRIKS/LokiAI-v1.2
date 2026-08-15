import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import type { DeviceFamily, ConnectorType } from '../../types';
import { X, Cpu, CheckCircle2, RefreshCw, Radio, HardDrive } from 'lucide-react';

interface ScannedPort {
  port: string;
  deviceType: string;
  detectedChip: string;
  baud: string;
  family: DeviceFamily;
  connector: ConnectorType;
}

export const DiscoverDeviceModal: React.FC = () => {
  const { isAddDeviceModalOpen, setIsAddDeviceModalOpen, addDiscoveredDevice } = useLoki();
  const [isScanning, setIsScanning] = useState(false);
  const [scannedPorts] = useState<ScannedPort[]>([
    {
      port: '/dev/ttyUSB1',
      deviceType: 'ESP32-S3 (WROOM-1)',
      detectedChip: 'ESP32-S3 (QFN56)',
      baud: '115200',
      family: 'ESP32-S3',
      connector: 'USB-CDC Serial',
    },
    {
      port: 'SWD-Probe-STLINKV3-004',
      deviceType: 'STM32-H7 Series Cortex-M7',
      detectedChip: 'STM32H743ZI Rev V',
      baud: '4000 kHz SWD',
      family: 'STM32-H7',
      connector: 'SWD / JTAG Probe',
    },
  ]);

  const [selectedScanIndex, setSelectedScanIndex] = useState(0);
  const [deviceName, setDeviceName] = useState('LOKI-S3-02');

  if (!isAddDeviceModalOpen) return null;

  const handleScan = () => {
    setIsScanning(true);
    setTimeout(() => {
      setIsScanning(false);
    }, 1200);
  };

  const handleConnect = () => {
    const selected = scannedPorts[selectedScanIndex];
    if (!selected) return;

    addDiscoveredDevice({
      name: deviceName || 'NEW-HARDWARE-NODE',
      family: selected.family,
      connector: selected.connector,
      transportPort: selected.port,
      manufacturer: selected.family === 'ESP32-S3' ? 'Espressif Systems' : 'STMicroelectronics',
      boardRevision: 'Rev 1.0 (Discovered)',
    });
  };

  return (
    <div
      id="discover-modal-backdrop"
      className="fixed inset-0 z-50 flex items-center justify-center bg-[#010101]/80 backdrop-blur-sm p-4 overflow-y-auto"
      role="dialog"
      aria-modal="true"
    >
      <div
        id="discover-modal-card"
        className="w-full max-w-xl bg-[#050505] border border-[#23221C] rounded-[14px] shadow-2xl overflow-hidden my-8"
      >
        {/* Header */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-b border-[#23221C]">
          <div className="flex items-center gap-3">
            <div className="p-2 rounded-[8px] bg-[#23221C] text-[#8AB55A]">
              <Cpu className="w-5 h-5" />
            </div>
            <div>
              <h2 className="text-base font-semibold text-[#E8D9A8]">
                Discover & Pair Hardware Node
              </h2>
              <p className="text-xs text-[#8A8577] font-mono">Loki Runtime Bus & Probe Scanner</p>
            </div>
          </div>
          <button
            onClick={() => setIsAddDeviceModalOpen(false)}
            className="p-1.5 rounded-[8px] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
          >
            <X className="w-4 h-4" />
          </button>
        </div>

        {/* Content Body */}
        <div className="p-6 space-y-5 text-xs text-[#E8D9A8]">
          {/* Scan Action Header */}
          <div className="flex items-center justify-between p-3 rounded-[8px] bg-[#0A0A08] border border-[#23221C]">
            <div className="flex items-center gap-2">
              <Radio className="w-4 h-4 text-[#8AB55A]" />
              <span className="text-[#E8D9A8] font-medium">Loki Runtime Gateway Probing</span>
            </div>
            <button
              onClick={handleScan}
              disabled={isScanning}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-[6px] bg-[#23221C] text-[#E8D9A8] hover:bg-[#38362D] transition-colors font-mono"
            >
              <RefreshCw
                className={`w-3.5 h-3.5 ${isScanning ? 'animate-spin text-[#8AB55A]' : ''}`}
              />
              {isScanning ? 'Scanning Buses...' : 'Rescan Ports'}
            </button>
          </div>

          {/* Discovered Candidates */}
          <div>
            <span className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-2">
              Available Unregistered Hardware Interfaces ({scannedPorts.length} found)
            </span>
            <div className="space-y-2">
              {scannedPorts.map((item, idx) => {
                const isSelected = selectedScanIndex === idx;
                return (
                  <div
                    key={idx}
                    onClick={() => {
                      setSelectedScanIndex(idx);
                      setDeviceName(idx === 0 ? 'LOKI-S3-02' : 'STM32-H7-CORE-05');
                    }}
                    className={`cursor-pointer p-3 rounded-[8px] border transition-all flex items-center justify-between ${
                      isSelected
                        ? 'bg-[#0A0A08] border-[#8AB55A] ring-1 ring-[#8AB55A]/40'
                        : 'bg-[#050505] border-[#23221C] hover:border-[#8A8577]'
                    }`}
                  >
                    <div className="flex items-center gap-3">
                      <HardDrive
                        className={`w-4 h-4 ${isSelected ? 'text-[#8AB55A]' : 'text-[#8A8577]'}`}
                      />
                      <div>
                        <div className="font-mono font-semibold text-xs text-[#E8D9A8]">
                          {item.deviceType}
                        </div>
                        <div className="text-[11px] text-[#8A8577] font-mono">
                          Port: {item.port} • Baud: {item.baud}
                        </div>
                      </div>
                    </div>

                    <span className="text-[10px] font-mono px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A]">
                      {item.connector}
                    </span>
                  </div>
                );
              })}
            </div>
          </div>

          {/* Node Naming & Config */}
          <div>
            <label className="text-xs font-semibold text-[#8A8577] uppercase tracking-wider block mb-1.5">
              Assigned Node Label / Name
            </label>
            <input
              type="text"
              value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              className="w-full px-3 py-2 rounded-[8px] bg-[#0A0A08] border border-[#23221C] font-mono text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
            />
          </div>
        </div>

        {/* Footer */}
        <div className="flex items-center justify-between px-6 py-4 bg-[#090909] border-t border-[#23221C]">
          <button
            onClick={() => setIsAddDeviceModalOpen(false)}
            className="px-4 py-2 rounded-[8px] bg-transparent border border-[#23221C] text-xs font-medium text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
          >
            Cancel
          </button>
          <button
            onClick={handleConnect}
            className="flex items-center gap-1.5 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-xs font-bold text-[#010101] hover:bg-[#A8C878] transition-colors"
          >
            <CheckCircle2 className="w-4 h-4" /> Connect & Register Node
          </button>
        </div>
      </div>
    </div>
  );
};
