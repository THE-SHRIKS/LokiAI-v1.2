import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import { DeviceFamily, ConnectorType } from '../../types';
import {
  Cpu,
  RefreshCw,
  Usb,
  Wifi,
  Bluetooth,
  ArrowRight,
  ArrowLeft,
  Settings2,
} from 'lucide-react';

interface DiscoveredHardware {
  id: string;
  name: string;
  family: DeviceFamily;
  connector: ConnectorType;
  transportPort: string;
  method: 'USB' | 'Serial' | 'Wi-Fi/Network' | 'Bluetooth' | 'ADB' | 'Gateway';
  supportTier: string;
  chipId: string;
  baudRate?: string;
  status: 'Ready to pair' | 'Driver required' | 'Paired';
}

const mockDiscoveredDevices: DiscoveredHardware[] = [
  {
    id: 'disc-01',
    name: 'Chamber-Node-Alpha',
    family: 'ESP32-S3',
    connector: 'USB-CDC Serial',
    transportPort: '/dev/ttyUSB0 (CP2102)',
    method: 'USB',
    supportTier: 'Tier-1 Certified',
    chipId: '0x307F-ESP32S3-WROOM-1',
    baudRate: '115200 8N1',
    status: 'Ready to pair',
  },
  {
    id: 'disc-02',
    name: 'H7-Core-Controller',
    family: 'STM32-H7',
    connector: 'SWD / JTAG Probe',
    transportPort: 'ST-Link/V3 (Bus 002:Dev 004)',
    method: 'USB',
    supportTier: 'Tier-1 Certified',
    chipId: '0x450-STM32H743ZI',
    status: 'Ready to pair',
  },
  {
    id: 'disc-03',
    name: 'Nordic-Sensor-Beacon',
    family: 'nRF5340',
    connector: 'BLE Gateway (mTLS)',
    transportPort: 'ble:dc:a6:32:88:14:02',
    method: 'Bluetooth',
    supportTier: 'Tier-2 Verified',
    chipId: '0x5340-nRF5340-QKAA',
    status: 'Ready to pair',
  },
  {
    id: 'disc-04',
    name: 'Edge-Gateway-Gateway',
    family: 'Raspberry Pi CM4',
    connector: 'Industrial Ethernet / MQTT',
    transportPort: '192.168.4.12:1883',
    method: 'Wi-Fi/Network',
    supportTier: 'Tier-1 Certified',
    chipId: 'BCM2711-CM4-4GB',
    status: 'Ready to pair',
  },
];

export const DeviceConnectView: React.FC = () => {
  const { updateOnboardingState, navigateToRoute, addToast } = useLoki();

  const [isScanning, setIsScanning] = useState(false);
  const [discoveredList, setDiscoveredList] = useState<DiscoveredHardware[]>(mockDiscoveredDevices);
  const [selectedDeviceId, setSelectedDeviceId] = useState<string>(mockDiscoveredDevices[0].id);
  const [showManualConnect, setShowManualConnect] = useState(false);
  const [manualPort, setManualPort] = useState('/dev/ttyUSB1');
  const [manualFamily, setManualFamily] = useState<DeviceFamily>('ESP32-S3');
  const [manualConnector, setManualConnector] = useState<ConnectorType>('USB-CDC Serial');

  const selectedDevice = discoveredList.find((d) => d.id === selectedDeviceId) || discoveredList[0];

  const handleScan = () => {
    setIsScanning(true);
    addToast({
      type: 'info',
      title: 'Bus Scan Initiated',
      message: 'Scanning USB-CDC, SWD probes, BLE mesh, and Ethernet MDNS...',
    });

    setTimeout(() => {
      setIsScanning(false);
      setDiscoveredList(mockDiscoveredDevices);
      addToast({
        type: 'success',
        title: 'Hardware Scan Complete',
        message: `Found ${mockDiscoveredDevices.length} compatible hardware targets on local bus.`,
      });
    }, 1200);
  };

  const handleProceedToHealthCheck = () => {
    updateOnboardingState({
      discoveredDevice: {
        id: selectedDevice.id,
        name: selectedDevice.name,
        family: selectedDevice.family,
        connector: selectedDevice.connector,
        transportPort: selectedDevice.transportPort,
        supportTier: selectedDevice.supportTier,
        healthStatus: 'Healthy',
        healthChecks: [
          {
            id: 'hc-1',
            name: 'Identity Verification',
            description: 'Silicon chip signature match & JTAG IDCODE',
            status: 'pending',
            details: selectedDevice.chipId,
          },
          {
            id: 'hc-2',
            name: 'Connection Validation',
            description: 'Transport ping, round-trip latency & packet loss',
            status: 'pending',
            details: `${selectedDevice.transportPort} (0.8ms latency)`,
          },
          {
            id: 'hc-3',
            name: 'Capability Discovery',
            description: 'Querying supported control primitives (reboot, logs, flash)',
            status: 'pending',
            details: '6 capabilities queried',
          },
          {
            id: 'hc-4',
            name: 'Firmware Identification',
            description: 'Bootloader hash & active image slot verification',
            status: 'pending',
            details: 'v1.4.0-release',
          },
          {
            id: 'hc-5',
            name: 'Heartbeat / Telemetry Response',
            description: 'Continuous 10Hz health stream watchdog check',
            status: 'pending',
            details: 'Watchdog margin: 388ms',
          },
          {
            id: 'hc-6',
            name: 'Available LokiAI Operations',
            description: 'Verify agent sandbox execution boundaries',
            status: 'pending',
            details: 'Full diagnostic & OTA scope',
          },
          {
            id: 'hc-7',
            name: 'Deployment & Rollback Support',
            description: 'Dual-bank flash partition table validation',
            status: 'pending',
            details: 'A/B Partitioning Active',
          },
        ],
      },
    });

    navigateToRoute('onboarding/device-health');
  };

  const handleAddManual = (e: React.FormEvent) => {
    e.preventDefault();
    const manualEntry: DiscoveredHardware = {
      id: `manual-${Date.now()}`,
      name: `Custom-${manualFamily}-Node`,
      family: manualFamily,
      connector: manualConnector,
      transportPort: manualPort,
      method: manualConnector.includes('USB') ? 'USB' : 'Serial',
      supportTier: 'Custom Hardware',
      chipId: `Custom-${Date.now().toString().slice(-4)}`,
      status: 'Ready to pair',
    };

    setDiscoveredList([manualEntry, ...discoveredList]);
    setSelectedDeviceId(manualEntry.id);
    setShowManualConnect(false);
    addToast({
      type: 'success',
      title: 'Manual Port Added',
      message: `Attached ${manualPort} (${manualFamily}).`,
    });
  };

  return (
    <div
      id="device-connect-screen"
      className="w-full max-w-4xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200"
    >
      <OnboardingStepper currentStep="device" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header */}
        <div className="flex flex-wrap items-center justify-between gap-3">
          <div>
            <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
              <Usb className="w-3.5 h-3.5" /> Step 5: Hardware Discovery & Transport
            </div>
            <h1 className="text-xl font-bold text-[#E8D9A8]">Connect your device</h1>
            <p className="text-xs text-[#8A8577] mt-1">
              Select an attached physical target or configure a custom hardware bridge.
            </p>
          </div>

          {/* Rescan Button */}
          <button
            id="scan-devices-btn"
            type="button"
            onClick={handleScan}
            disabled={isScanning}
            className="flex items-center gap-2 px-3.5 py-1.5 rounded-[8px] bg-[#050505] border border-[#23221C] hover:border-[#8AB55A]/50 text-xs font-semibold text-[#E8D9A8] transition-colors"
          >
            <RefreshCw
              className={`w-3.5 h-3.5 text-[#8AB55A] ${isScanning ? 'animate-spin' : ''}`}
            />
            <span>{isScanning ? 'Scanning bus...' : 'Scan for devices'}</span>
          </button>
        </div>

        {/* Supported Connection Methods Banner */}
        <div className="p-3 rounded-[8px] bg-[#050505] border border-[#23221C] flex flex-wrap items-center justify-between gap-3 text-xs">
          <span className="text-[#8A8577] font-mono text-[11px]">Active Daemon Connectors:</span>
          <div className="flex flex-wrap gap-2 text-[11px] font-mono">
            <span className="px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A] flex items-center gap-1">
              <Usb className="w-3 h-3" /> USB/CDC
            </span>
            <span className="px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A] flex items-center gap-1">
              <Cpu className="w-3 h-3" /> SWD/JTAG
            </span>
            <span className="px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A] flex items-center gap-1">
              <Bluetooth className="w-3 h-3" /> BLE Mesh
            </span>
            <span className="px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A] flex items-center gap-1">
              <Wifi className="w-3 h-3" /> Ethernet
            </span>
          </div>
        </div>

        {/* Discovered Devices Table / List */}
        <div className="space-y-2">
          <div className="flex items-center justify-between text-[11px] font-mono uppercase text-[#8A8577] px-1">
            <span>Discovered Targets ({discoveredList.length})</span>
            <span>Support Tier</span>
          </div>

          <div className="space-y-2">
            {discoveredList.map((dev) => {
              const isSelected = selectedDeviceId === dev.id;

              return (
                <button
                  key={dev.id}
                  type="button"
                  onClick={() => setSelectedDeviceId(dev.id)}
                  className={`w-full p-4 rounded-[10px] border text-left flex items-center justify-between gap-4 transition-all ${
                    isSelected
                      ? 'bg-[#6B8C42]/10 border-[#8AB55A] shadow-md ring-1 ring-[#8AB55A]'
                      : 'bg-[#050505] border-[#23221C] hover:border-[#8AB55A]/40'
                  }`}
                >
                  <div className="flex items-center gap-3.5 min-w-0">
                    <div
                      className={`w-9 h-9 rounded-[8px] flex items-center justify-center shrink-0 ${
                        isSelected
                          ? 'bg-[#6B8C42]/20 border border-[#8AB55A] text-[#8AB55A]'
                          : 'bg-[#23221C] text-[#8A8577]'
                      }`}
                    >
                      <Cpu className="w-5 h-5" />
                    </div>

                    <div className="min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="font-bold text-xs text-[#E8D9A8] font-mono">
                          {dev.name}
                        </span>
                        <span className="text-[10px] px-1.5 py-0.2 rounded bg-[#0A0A08] border border-[#23221C] text-[#8AB55A] font-mono">
                          {dev.family}
                        </span>
                      </div>
                      <div className="text-[11px] text-[#8A8577] font-mono mt-0.5 truncate">
                        Port: <strong className="text-[#E8D9A8]">{dev.transportPort}</strong> •
                        Connector: {dev.connector}
                      </div>
                    </div>
                  </div>

                  <div className="flex items-center gap-3 shrink-0">
                    <span className="hidden sm:inline-block text-[10px] font-mono text-[#8AB55A] px-2 py-0.5 rounded bg-[#0A0A08] border border-[#23221C]">
                      {dev.supportTier}
                    </span>
                    <div
                      className={`w-5 h-5 rounded-full border flex items-center justify-center ${
                        isSelected
                          ? 'border-[#8AB55A] bg-[#8AB55A] text-[#010101]'
                          : 'border-[#23221C]'
                      }`}
                    >
                      {isSelected && <span className="text-xs font-bold">✓</span>}
                    </div>
                  </div>
                </button>
              );
            })}
          </div>
        </div>

        {/* Manual Connection Trigger / Form */}
        <div className="pt-2">
          {!showManualConnect ? (
            <button
              type="button"
              onClick={() => setShowManualConnect(true)}
              className="text-xs font-mono text-[#8AB55A] hover:underline flex items-center gap-1.5"
            >
              <Settings2 className="w-3.5 h-3.5" /> Configure custom port / IP connection manually
            </button>
          ) : (
            <form
              onSubmit={handleAddManual}
              className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-3 animate-in fade-in"
            >
              <div className="flex items-center justify-between text-xs font-semibold text-[#E8D9A8]">
                <span>Manual Hardware Connector</span>
                <button
                  type="button"
                  onClick={() => setShowManualConnect(false)}
                  className="text-[11px] text-[#8A8577] hover:text-[#E8D9A8]"
                >
                  Cancel
                </button>
              </div>

              <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 text-xs">
                <div>
                  <label className="block text-[11px] text-[#8A8577] mb-1">Target Family</label>
                  <select
                    value={manualFamily}
                    onChange={(e) => setManualFamily(e.target.value as any)}
                    className="w-full p-2 rounded bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] text-xs font-mono"
                  >
                    <option value="ESP32-S3">ESP32-S3</option>
                    <option value="STM32-H7">STM32-H7</option>
                    <option value="nRF5340">nRF5340</option>
                    <option value="Raspberry Pi CM4">Raspberry Pi CM4</option>
                    <option value="i.MX8M-Plus">i.MX8M-Plus</option>
                  </select>
                </div>

                <div>
                  <label className="block text-[11px] text-[#8A8577] mb-1">Connector</label>
                  <select
                    value={manualConnector}
                    onChange={(e) => setManualConnector(e.target.value as any)}
                    className="w-full p-2 rounded bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] text-xs font-mono"
                  >
                    <option value="USB-CDC Serial">USB-CDC Serial</option>
                    <option value="SWD / JTAG Probe">SWD / JTAG Probe</option>
                    <option value="Industrial Ethernet / MQTT">Industrial Ethernet / MQTT</option>
                    <option value="BLE Gateway (mTLS)">BLE Gateway (mTLS)</option>
                  </select>
                </div>

                <div>
                  <label className="block text-[11px] text-[#8A8577] mb-1">Port / IP Address</label>
                  <input
                    type="text"
                    value={manualPort}
                    onChange={(e) => setManualPort(e.target.value)}
                    placeholder="/dev/ttyUSB1 or 192.168.1.50"
                    className="w-full p-2 rounded bg-[#0A0A08] border border-[#23221C] text-[#E8D9A8] text-xs font-mono"
                  />
                </div>
              </div>

              <div className="flex justify-end pt-1">
                <button
                  type="submit"
                  className="px-3 py-1.5 rounded-[6px] bg-[#23221C] hover:bg-[#38362D] text-[#E8D9A8] text-xs font-semibold"
                >
                  Add Custom Target
                </button>
              </div>
            </form>
          )}
        </div>

        {/* Footer Navigation */}
        <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
          <button
            type="button"
            onClick={() => navigateToRoute('onboarding/device-choice')}
            className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Back
          </button>

          <button
            id="run-health-check-btn"
            type="button"
            onClick={handleProceedToHealthCheck}
            className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors shadow-sm"
          >
            <span>Proceed to Health Check</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};
