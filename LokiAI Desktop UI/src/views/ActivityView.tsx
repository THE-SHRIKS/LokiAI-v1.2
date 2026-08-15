import React, { useState, useMemo } from 'react';
import { useLoki } from '../context/LokiContext';
import { SeverityBadge } from '../components/common/SeverityBadge';
import {
  History,
  Search,
  Filter,
  Download,
  FileCode,
  ShieldCheck,
  CheckCircle2,
  XCircle,
  ExternalLink,
  Layers,
} from 'lucide-react';

export const ActivityView: React.FC = () => {
  const { activityEvents, openInspector, navigateTo } = useLoki();
  const [query, setQuery] = useState('');
  const [riskFilter, setRiskFilter] = useState<string>('ALL');
  const [resultFilter, setResultFilter] = useState<string>('ALL');

  const filteredActivities = useMemo(() => {
    return activityEvents.filter((act) => {
      if (riskFilter !== 'ALL' && act.riskLevel !== riskFilter) return false;
      if (resultFilter !== 'ALL' && act.result !== resultFilter) return false;
      if (query.trim()) {
        const q = query.toLowerCase();
        const summaryText = act.actionSummary || act.summary || '';
        return (
          summaryText.toLowerCase().includes(q) ||
          act.actor.name.toLowerCase().includes(q) ||
          act.resourceName.toLowerCase().includes(q) ||
          act.eventType.toLowerCase().includes(q) ||
          (act.correlationId && act.correlationId.toLowerCase().includes(q))
        );
      }
      return true;
    });
  }, [activityEvents, riskFilter, resultFilter, query]);

  const handleExportJSON = () => {
    const dataStr = 'data:text/json;charset=utf-8,' + encodeURIComponent(JSON.stringify(filteredActivities, null, 2));
    const downloadAnchor = document.createElement('a');
    downloadAnchor.setAttribute('href', dataStr);
    downloadAnchor.setAttribute('download', `loki-activity-audit-${new Date().toISOString()}.json`);
    document.body.appendChild(downloadAnchor);
    downloadAnchor.click();
    downloadAnchor.remove();
  };

  return (
    <div id="activity-view" className="space-y-6">
      {/* Top Banner & Multi-Filters */}
      <div className="flex flex-wrap items-center justify-between gap-4 p-4 rounded-[10px] bg-[#050505] border border-[#23221C]">
        <div className="flex flex-wrap items-center gap-3">
          {/* Search */}
          <div className="relative">
            <Search className="w-3.5 h-3.5 text-[#8A8577] absolute left-3 top-2.5" />
            <input
              type="text"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search audit trail by actor, device, ID..."
              className="pl-8 pr-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] placeholder-[#8A8577] focus:outline-none focus:border-[#8AB55A] w-52 sm:w-64 font-mono"
            />
          </div>

          {/* Risk Filter */}
          <select
            value={riskFilter}
            onChange={(e) => setRiskFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Risk Levels</option>
            <option value="Critical">Critical</option>
            <option value="High">High</option>
            <option value="Medium">Medium</option>
            <option value="Low">Low</option>
          </select>

          {/* Result Filter */}
          <select
            value={resultFilter}
            onChange={(e) => setResultFilter(e.target.value)}
            className="px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] text-xs font-mono text-[#E8D9A8] focus:outline-none focus:border-[#8AB55A]"
          >
            <option value="ALL">All Results</option>
            <option value="Success">Success</option>
            <option value="Denied">Denied</option>
            <option value="Failed">Failed</option>
            <option value="In-Progress">In-Progress</option>
          </select>
        </div>

        <button
          onClick={handleExportJSON}
          className="flex items-center gap-1.5 px-3 py-1.5 rounded-[8px] bg-[#0A0A08] border border-[#23221C] hover:border-[#8AB55A]/50 text-xs font-mono text-[#8AB55A] hover:bg-[#23221C] transition-colors"
        >
          <Download className="w-3.5 h-3.5" /> Export Audit JSON
        </button>
      </div>

      {/* Activity Table */}
      <div className="bg-[#050505] border border-[#23221C] rounded-[10px] overflow-hidden">
        <div className="overflow-x-auto">
          <table className="w-full text-left text-xs font-mono">
            <thead className="bg-[#090909] border-b border-[#23221C] text-[#8A8577] text-[11px]">
              <tr>
                <th className="py-3 px-4">Timestamp (UTC)</th>
                <th className="py-3 px-4">Actor</th>
                <th className="py-3 px-4">Action & Resource</th>
                <th className="py-3 px-4">Risk Level</th>
                <th className="py-3 px-4">Result</th>
                <th className="py-3 px-4">Correlation</th>
                <th className="py-3 px-4 text-right">Inspect</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-[#23221C] text-[#E8D9A8]">
              {filteredActivities.map((act) => (
                <tr
                  key={act.id}
                  id={`activity-row-${act.id}`}
                  onClick={() => openInspector('activity', act)}
                  className="hover:bg-[#0A0A08] cursor-pointer transition-colors group"
                >
                  <td className="py-3 px-4 text-[#8A8577] whitespace-nowrap">{act.timestamp}</td>
                  <td className="py-3 px-4">
                    <div className="font-semibold text-[#E8D9A8]">{act.actor.name}</div>
                    <div className="text-[10px] text-[#8A8577] capitalize">{act.actor.type}</div>
                  </td>
                  <td className="py-3 px-4">
                    <div className="font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors line-clamp-1">
                      {act.actionSummary || act.summary}
                    </div>
                    <div className="text-[10px] text-[#8A8577]">
                      {act.resourceType}: <strong className="text-[#E8D9A8] font-mono">{act.resourceName}</strong>
                    </div>
                  </td>
                  <td className="py-3 px-4">
                    <SeverityBadge severity={act.riskLevel} />
                  </td>
                  <td className="py-3 px-4">
                    <span
                      className={`px-2 py-0.5 rounded text-[10px] font-bold ${
                        act.result === 'Success'
                          ? 'bg-[#6B8C42]/20 text-[#8AB55A]'
                          : act.result === 'Denied'
                          ? 'bg-[#23221C] text-[#E8D9A8]'
                          : 'bg-[#CC272E]/20 text-[#CC272E]'
                      }`}
                    >
                      {act.result}
                    </span>
                  </td>
                  <td className="py-3 px-4 text-[#8A8577]">
                    {act.correlationId ? (
                      <span className="px-1.5 py-0.5 rounded bg-[#0A0A08] border border-[#23221C] text-[10px] text-[#8AB55A]">
                        {act.correlationId}
                      </span>
                    ) : (
                      '—'
                    )}
                  </td>
                  <td className="py-3 px-4 text-right">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        openInspector('activity', act);
                      }}
                      className="px-2.5 py-1 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-[11px] text-[#8AB55A] hover:bg-[#23221C] transition-colors"
                    >
                      Payload
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
