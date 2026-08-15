import React, { useState, useMemo } from 'react';
import { useLoki } from '../../context/LokiContext';
import { Table, Activity, AlertTriangle } from 'lucide-react';

interface TelemetryChartProps {
  deviceId?: string;
  title?: string;
  className?: string;
}

export const TelemetryChart: React.FC<TelemetryChartProps> = ({
  // Not yet consumed by any caller (single shared telemetry stream today) —
  // kept in the public prop shape for the per-device scoping that MON-001
  // introduces; renamed to signal it's intentionally unused for now rather
  // than silently dropped.
  deviceId: _deviceId,
  title = 'Recorded Hardware Telemetry',
  className = '',
}) => {
  const { telemetryHistory } = useLoki();
  const [selectedMetric, setSelectedMetric] = useState<
    'voltage' | 'temperature' | 'cpuUsage' | 'watchdogMarginMs'
  >('voltage');
  const [timeRange, setTimeRange] = useState<'15m' | '1h' | '6h' | '24h'>('1h');
  const [viewMode, setViewMode] = useState<'chart' | 'table'>('chart');

  interface MetricConfigEntry {
    label: string;
    unit: string;
    min: number;
    max: number;
    // Mutually exclusive per metric (a brownout floor vs. a thermal/load
    // ceiling) — both stay optional on the shared shape rather than each
    // literal only declaring the one it uses.
    thresholdLow?: number;
    thresholdHigh?: number;
    thresholdLabel: string;
    color: string;
    stroke: string;
    fill: string;
  }

  const metricConfig: Record<
    'voltage' | 'temperature' | 'cpuUsage' | 'watchdogMarginMs',
    MetricConfigEntry
  > = {
    voltage: {
      label: 'VCC Rail Voltage',
      unit: 'V',
      min: 3.0,
      max: 3.5,
      thresholdLow: 3.15,
      thresholdLabel: 'Brownout Threshold (3.15V)',
      color: '#8AB55A',
      stroke: '#8AB55A',
      fill: 'rgba(138, 181, 90, 0.1)',
    },
    temperature: {
      label: 'Core Junction Temp',
      unit: '°C',
      min: 20,
      max: 80,
      thresholdHigh: 75,
      thresholdLabel: 'Thermal Limit (75°C)',
      color: '#E8D9A8',
      stroke: '#E8D9A8',
      fill: 'rgba(232, 217, 168, 0.1)',
    },
    cpuUsage: {
      label: 'Compute Core Load',
      unit: '%',
      min: 0,
      max: 100,
      thresholdHigh: 85,
      thresholdLabel: 'High Load (85%)',
      color: '#6B8C42',
      stroke: '#6B8C42',
      fill: 'rgba(107, 140, 66, 0.1)',
    },
    watchdogMarginMs: {
      label: 'Watchdog Margin',
      unit: 'ms',
      min: 0,
      max: 500,
      thresholdLow: 100,
      thresholdLabel: 'Starvation Limit (100ms)',
      color: '#A8C878',
      stroke: '#A8C878',
      fill: 'rgba(168, 200, 120, 0.1)',
    },
  };

  const config = metricConfig[selectedMetric];

  // Filter time range points
  const points = useMemo(() => {
    let sliceCount = 14;
    if (timeRange === '15m') sliceCount = 6;
    if (timeRange === '1h') sliceCount = 14;
    if (timeRange === '6h') sliceCount = 20;
    if (timeRange === '24h') sliceCount = 25;
    return telemetryHistory.slice(-sliceCount);
  }, [telemetryHistory, timeRange]);

  // Chart dimensions & SVG path calculation
  const svgWidth = 700;
  const svgHeight = 220;
  const paddingLeft = 45;
  const paddingRight = 20;
  const paddingTop = 25;
  const paddingBottom = 30;

  const chartW = svgWidth - paddingLeft - paddingRight;
  const chartH = svgHeight - paddingTop - paddingBottom;

  const yVal = (val: number) => {
    const clamped = Math.max(config.min, Math.min(config.max, val));
    const ratio = (clamped - config.min) / (config.max - config.min);
    return paddingTop + chartH * (1 - ratio);
  };

  const xVal = (index: number) => {
    if (points.length <= 1) return paddingLeft;
    return paddingLeft + (index / (points.length - 1)) * chartW;
  };

  // Build SVG Path
  const linePath = useMemo(() => {
    if (points.length === 0) return '';
    return points
      .map((p, i) => {
        const x = xVal(i);
        const y = yVal(p[selectedMetric]);
        return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
      })
      .join(' ');
  }, [points, selectedMetric, config]);

  const areaPath = useMemo(() => {
    if (points.length === 0) return '';
    const firstX = xVal(0);
    const lastX = xVal(points.length - 1);
    const baselineY = yVal(config.min);
    return `${linePath} L ${lastX} ${baselineY} L ${firstX} ${baselineY} Z`;
  }, [linePath, points, config]);

  const thresholdY = config.thresholdLow
    ? yVal(config.thresholdLow)
    : config.thresholdHigh
      ? yVal(config.thresholdHigh)
      : null;

  return (
    <div
      id="telemetry-chart-container"
      className={`bg-[#050505] border border-[#23221C] rounded-[10px] p-4 flex flex-col justify-between ${className}`}
    >
      {/* Top Controls */}
      <div className="flex flex-wrap items-center justify-between gap-3 pb-3 border-b border-[#23221C]">
        <div className="flex items-center gap-2">
          <Activity className="w-4 h-4 text-[#8AB55A]" />
          <h3 className="text-xs font-semibold text-[#E8D9A8] uppercase tracking-wider">{title}</h3>
        </div>

        <div className="flex flex-wrap items-center gap-2">
          {/* Metric Selector Tabs */}
          <div className="flex items-center rounded-[6px] bg-[#0A0A08] border border-[#23221C] p-0.5">
            {(['voltage', 'temperature', 'cpuUsage', 'watchdogMarginMs'] as const).map((key) => (
              <button
                key={key}
                onClick={() => setSelectedMetric(key)}
                className={`px-2.5 py-1 text-xs rounded-[4px] font-medium transition-all ${
                  selectedMetric === key
                    ? 'bg-[#23221C] text-[#E8D9A8] border border-[#8AB55A]/40'
                    : 'text-[#8A8577] hover:text-[#E8D9A8]'
                }`}
              >
                {metricConfig[key].label.split(' ')[0]}
              </button>
            ))}
          </div>

          {/* Time Range Selector */}
          <div className="flex items-center rounded-[6px] bg-[#0A0A08] border border-[#23221C] p-0.5 font-mono text-xs">
            {(['15m', '1h', '6h', '24h'] as const).map((r) => (
              <button
                key={r}
                onClick={() => setTimeRange(r)}
                className={`px-2 py-0.5 rounded-[4px] transition-all ${
                  timeRange === r
                    ? 'bg-[#6B8C42]/20 text-[#8AB55A] font-semibold'
                    : 'text-[#8A8577] hover:text-[#E8D9A8]'
                }`}
              >
                {r}
              </button>
            ))}
          </div>

          {/* Table View Toggle (Accessible requirement) */}
          <button
            onClick={() => setViewMode(viewMode === 'chart' ? 'table' : 'chart')}
            className="p-1.5 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[#8A8577] hover:text-[#E8D9A8] transition-colors"
            title={
              viewMode === 'chart' ? 'Switch to accessible table view' : 'Switch to chart view'
            }
            aria-label="Toggle table or chart view"
          >
            {viewMode === 'chart' ? (
              <Table className="w-3.5 h-3.5" />
            ) : (
              <Activity className="w-3.5 h-3.5" />
            )}
          </button>
        </div>
      </div>

      {/* Main Visual Display */}
      {viewMode === 'chart' ? (
        <div className="pt-3 relative overflow-hidden">
          <div className="flex items-center justify-between text-xs font-mono text-[#8A8577] mb-1">
            <span>
              Latest:{' '}
              <strong className="text-[#E8D9A8]">
                {points[points.length - 1]?.[selectedMetric]} {config.unit}
              </strong>
            </span>
            <span className="text-[11px]">Source: Verified Runtime Gateway (1s sample rate)</span>
          </div>

          <div className="w-full overflow-x-auto">
            <svg
              viewBox={`0 0 ${svgWidth} ${svgHeight}`}
              className="w-full h-44 text-xs font-mono"
              role="img"
              aria-label={`${config.label} telemetry chart over time`}
            >
              {/* Grid Lines */}
              <line
                x1={paddingLeft}
                y1={paddingTop}
                x2={svgWidth - paddingRight}
                y2={paddingTop}
                stroke="#23221C"
                strokeDasharray="3 3"
              />
              <line
                x1={paddingLeft}
                y1={paddingTop + chartH / 2}
                x2={svgWidth - paddingRight}
                y2={paddingTop + chartH / 2}
                stroke="#23221C"
                strokeDasharray="3 3"
              />
              <line
                x1={paddingLeft}
                y1={paddingTop + chartH}
                x2={svgWidth - paddingRight}
                y2={paddingTop + chartH}
                stroke="#23221C"
              />

              {/* Threshold Line overlay */}
              {thresholdY !== null && (
                <g>
                  <line
                    x1={paddingLeft}
                    y1={thresholdY}
                    x2={svgWidth - paddingRight}
                    y2={thresholdY}
                    stroke="#CC272E"
                    strokeDasharray="4 4"
                    strokeWidth="1.5"
                  />
                  <text
                    x={svgWidth - paddingRight - 8}
                    y={thresholdY - 5}
                    textAnchor="end"
                    fill="#CC272E"
                    className="text-[9px] font-mono font-medium"
                  >
                    {config.thresholdLabel}
                  </text>
                </g>
              )}

              {/* Y-Axis Labels */}
              <text
                x={paddingLeft - 8}
                y={paddingTop + 4}
                textAnchor="end"
                fill="#8A8577"
                className="text-[10px]"
              >
                {config.max}
                {config.unit}
              </text>
              <text
                x={paddingLeft - 8}
                y={paddingTop + chartH / 2 + 4}
                textAnchor="end"
                fill="#8A8577"
                className="text-[10px]"
              >
                {((config.min + config.max) / 2).toFixed(1)}
                {config.unit}
              </text>
              <text
                x={paddingLeft - 8}
                y={paddingTop + chartH}
                textAnchor="end"
                fill="#8A8577"
                className="text-[10px]"
              >
                {config.min}
                {config.unit}
              </text>

              {/* Area Fill */}
              <path d={areaPath} fill={config.fill} />

              {/* Series Line */}
              <path
                d={linePath}
                fill="none"
                stroke={config.stroke}
                strokeWidth="2"
                strokeLinecap="round"
              />

              {/* Data points & time markers */}
              {points.map((pt, idx) => {
                const x = xVal(idx);
                const y = yVal(pt[selectedMetric]);
                const isViolation =
                  (config.thresholdLow && pt[selectedMetric] < config.thresholdLow) ||
                  (config.thresholdHigh && pt[selectedMetric] > config.thresholdHigh);

                return (
                  <g key={idx} className="group">
                    <circle
                      cx={x}
                      cy={y}
                      r={isViolation ? 4 : 2.5}
                      fill={isViolation ? '#CC272E' : config.stroke}
                      stroke="#050505"
                      strokeWidth="1.5"
                    />
                    {/* Time ticks for selected points */}
                    {idx % Math.ceil(points.length / 5) === 0 && (
                      <text
                        x={x}
                        y={svgHeight - 8}
                        textAnchor="middle"
                        fill="#8A8577"
                        className="text-[10px]"
                      >
                        {pt.timeLabel}
                      </text>
                    )}
                  </g>
                );
              })}
            </svg>
          </div>
        </div>
      ) : (
        /* Accessible Table Alternative */
        <div className="pt-3 max-h-52 overflow-y-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-1.5 px-2">Timestamp</th>
                <th className="py-1.5 px-2">
                  {config.label} ({config.unit})
                </th>
                <th className="py-1.5 px-2">Status Condition</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {points.map((p, i) => {
                const isViolation =
                  (config.thresholdLow && p[selectedMetric] < config.thresholdLow) ||
                  (config.thresholdHigh && p[selectedMetric] > config.thresholdHigh);
                return (
                  <tr key={i} className="hover:bg-[#0A0A08]">
                    <td className="py-1.5 px-2 text-[#8A8577]">{p.timestamp}</td>
                    <td className="py-1.5 px-2 font-semibold">
                      {p[selectedMetric]} {config.unit}
                    </td>
                    <td className="py-1.5 px-2">
                      {isViolation ? (
                        <span className="text-[#CC272E] font-medium flex items-center gap-1">
                          <AlertTriangle className="w-3 h-3" /> Threshold Violated
                        </span>
                      ) : (
                        <span className="text-[#8AB55A]">Nominal</span>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
};
