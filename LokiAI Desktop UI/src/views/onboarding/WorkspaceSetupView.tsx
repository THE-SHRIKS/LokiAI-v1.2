import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import {
  Briefcase,
  ArrowRight,
  ArrowLeft,
  CheckCircle2,
  AlertCircle,
  FolderGit2,
} from 'lucide-react';

export const WorkspaceSetupView: React.FC = () => {
  const { onboardingState, updateOnboardingState, navigateToRoute, workspaces, addToast } =
    useLoki();

  const [mode, setMode] = useState<'create' | 'existing'>('create');
  const [name, setName] = useState(onboardingState.workspace.name || '');
  const [description, setDescription] = useState(onboardingState.workspace.description || '');
  const [environment, setEnvironment] = useState<'development' | 'staging' | 'production'>(
    onboardingState.workspace.environment || 'development',
  );
  const [repoPath, setRepoPath] = useState(
    onboardingState.workspace.repoPath || '~/firmware/loki-chamber-core',
  );
  const [connectRepoLater, setConnectRepoLater] = useState(
    onboardingState.workspace.connectRepoLater || false,
  );
  const [selectedExistingId, setSelectedExistingId] = useState(workspaces[0]?.id || '');
  const [nameError, setNameError] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const validate = () => {
    if (mode === 'create') {
      if (!name.trim()) {
        setNameError('Workspace name is required.');
        return false;
      }
      if (name.trim().length < 3) {
        setNameError('Workspace name must be at least 3 characters.');
        return false;
      }
      if (!/^[a-zA-Z0-9-_ ]+$/.test(name.trim())) {
        setNameError(
          'Workspace name may only contain alphanumeric characters, hyphens, and underscores.',
        );
        return false;
      }
    }
    setNameError('');
    return true;
  };

  const handleCreateOrSelect = (e: React.FormEvent) => {
    e.preventDefault();
    if (!validate()) return;

    setIsSubmitting(true);
    setTimeout(() => {
      if (mode === 'create') {
        updateOnboardingState({
          currentStep: 'agent',
          workspace: {
            name: name.trim(),
            description: description.trim(),
            environment,
            repoPath: connectRepoLater ? '' : repoPath.trim(),
            connectRepoLater,
            isExisting: false,
          },
        });
        addToast({
          type: 'success',
          title: 'Workspace Initialized',
          message: `Created "${name.trim()}" in ${environment} environment.`,
        });
      } else {
        const found = workspaces.find((w) => w.id === selectedExistingId) || workspaces[0];
        const mappedEnv = found.environment === 'lab' ? 'development' : found.environment;
        updateOnboardingState({
          currentStep: 'agent',
          workspace: {
            name: found.name,
            description: 'Existing shared cluster workspace',
            environment: mappedEnv,
            repoPath: '~/repos/' + found.name.toLowerCase(),
            connectRepoLater: false,
            isExisting: true,
            existingWorkspaceId: found.id,
          },
        });
        addToast({
          type: 'info',
          title: 'Existing Workspace Selected',
          message: `Attached to "${found.name}".`,
        });
      }

      setIsSubmitting(false);
      navigateToRoute('onboarding/agent');
    }, 400);
  };

  return (
    <div
      id="workspace-setup-screen"
      className="w-full max-w-3xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200"
    >
      <OnboardingStepper currentStep="workspace" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header Title */}
        <div>
          <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
            <Briefcase className="w-3.5 h-3.5" /> Step 2: Workspace Environment
          </div>
          <h1 className="text-xl font-bold text-[#E8D9A8]">Set up your workspace</h1>
          <p className="text-xs text-[#8A8577] mt-1.5 leading-relaxed">
            A workspace organizes your target hardware, coding agents, automated incident telemetry,
            firmware builds, and verification history.
          </p>
        </div>

        {/* Tab Toggle: Create New vs Use Existing */}
        <div className="flex p-1 rounded-[8px] bg-[#050505] border border-[#23221C] max-w-sm">
          <button
            type="button"
            onClick={() => setMode('create')}
            className={`flex-1 py-1.5 px-3 rounded-[6px] text-xs font-semibold transition-colors ${
              mode === 'create'
                ? 'bg-[#23221C] text-[#E8D9A8] shadow-sm'
                : 'text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Create new workspace
          </button>
          <button
            type="button"
            onClick={() => setMode('existing')}
            className={`flex-1 py-1.5 px-3 rounded-[6px] text-xs font-semibold transition-colors ${
              mode === 'existing'
                ? 'bg-[#23221C] text-[#E8D9A8] shadow-sm'
                : 'text-[#8A8577] hover:text-[#E8D9A8]'
            }`}
          >
            Use existing workspace
          </button>
        </div>

        {/* Mode Form */}
        <form onSubmit={handleCreateOrSelect} className="space-y-5">
          {mode === 'create' ? (
            <div className="space-y-4">
              {/* Name */}
              <div>
                <label className="block text-xs font-semibold text-[#E8D9A8] mb-1.5">
                  Workspace Name <span className="text-[#CC272E]">*</span>
                </label>
                <input
                  id="workspace-name-input"
                  type="text"
                  value={name}
                  onChange={(e) => {
                    setName(e.target.value);
                    if (nameError) setNameError('');
                  }}
                  placeholder="e.g. Chamber-Alpha, Robot-Arm-Core"
                  className={`w-full px-3.5 py-2.5 rounded-[8px] bg-[#050505] border text-xs text-[#E8D9A8] font-mono focus:outline-none transition-colors ${
                    nameError
                      ? 'border-[#CC272E] focus:border-[#CC272E]'
                      : 'border-[#23221C] focus:border-[#8AB55A]'
                  }`}
                />
                {nameError ? (
                  <p className="mt-1.5 text-[11px] text-[#CC272E] flex items-center gap-1 font-mono">
                    <AlertCircle className="w-3 h-3" /> {nameError}
                  </p>
                ) : (
                  <p className="mt-1 text-[10px] text-[#8A8577] font-mono">
                    Used to isolate device telemetry channels and approval scopes.
                  </p>
                )}
              </div>

              {/* Description */}
              <div>
                <label className="block text-xs font-semibold text-[#E8D9A8] mb-1.5">
                  Description <span className="text-[#8A8577] font-normal">(optional)</span>
                </label>
                <input
                  type="text"
                  value={description}
                  onChange={(e) => setDescription(e.target.value)}
                  placeholder="e.g. Active validation chamber for STM32 and ESP32 hardware"
                  className="w-full px-3.5 py-2.5 rounded-[8px] bg-[#050505] border border-[#23221C] text-xs text-[#E8D9A8] focus:border-[#8AB55A] focus:outline-none"
                />
              </div>

              {/* Environment Selection */}
              <div>
                <label className="block text-xs font-semibold text-[#E8D9A8] mb-1.5">
                  Environment Tier
                </label>
                <div className="grid grid-cols-1 sm:grid-cols-3 gap-3">
                  {[
                    {
                      id: 'development',
                      label: 'Development',
                      desc: 'Fast iterate • Auto-retry OTA',
                    },
                    { id: 'staging', label: 'Staging', desc: 'Pre-flight gates • 60s soak' },
                    {
                      id: 'production',
                      label: 'Production',
                      desc: 'Strict human approval required',
                    },
                  ].map((env) => (
                    <button
                      key={env.id}
                      type="button"
                      onClick={() => setEnvironment(env.id as any)}
                      className={`p-3 rounded-[8px] border text-left transition-all ${
                        environment === env.id
                          ? 'bg-[#6B8C42]/10 border-[#8AB55A] text-[#E8D9A8]'
                          : 'bg-[#050505] border-[#23221C] text-[#8A8577] hover:border-[#8AB55A]/40'
                      }`}
                    >
                      <div className="flex items-center justify-between mb-1">
                        <span className="font-bold text-xs capitalize text-[#E8D9A8]">
                          {env.label}
                        </span>
                        {environment === env.id && (
                          <CheckCircle2 className="w-3.5 h-3.5 text-[#8AB55A]" />
                        )}
                      </div>
                      <span className="text-[10px] text-[#8A8577] block font-mono">{env.desc}</span>
                    </button>
                  ))}
                </div>
              </div>

              {/* Project / Repository Location */}
              <div className="p-4 rounded-[8px] bg-[#050505] border border-[#23221C] space-y-3">
                <div className="flex items-center justify-between">
                  <span className="text-xs font-semibold text-[#E8D9A8] flex items-center gap-1.5">
                    <FolderGit2 className="w-4 h-4 text-[#8AB55A]" /> Project / Firmware Source Path
                  </span>
                  <label className="flex items-center gap-1.5 text-[11px] text-[#8A8577] cursor-pointer">
                    <input
                      type="checkbox"
                      checked={connectRepoLater}
                      onChange={(e) => setConnectRepoLater(e.target.checked)}
                      className="rounded border-[#23221C] text-[#8AB55A] focus:ring-0"
                    />
                    <span>Connect later</span>
                  </label>
                </div>

                {!connectRepoLater && (
                  <div>
                    <input
                      type="text"
                      value={repoPath}
                      onChange={(e) => setRepoPath(e.target.value)}
                      placeholder="~/firmware/loki-chamber-core"
                      className="w-full px-3 py-2 rounded-[6px] bg-[#0A0A08] border border-[#23221C] text-xs text-[#E8D9A8] font-mono focus:border-[#8AB55A] focus:outline-none"
                    />
                    <span className="text-[10px] text-[#8A8577] font-mono mt-1 block">
                      Local repo path where coding agents will perform diagnostic inspections and
                      builds.
                    </span>
                  </div>
                )}
              </div>
            </div>
          ) : (
            /* Existing Workspaces Selection */
            <div className="space-y-3">
              <label className="block text-xs font-semibold text-[#E8D9A8]">
                Select Existing Cluster Workspace
              </label>
              <div className="space-y-2">
                {workspaces.map((ws) => (
                  <button
                    key={ws.id}
                    type="button"
                    onClick={() => setSelectedExistingId(ws.id)}
                    className={`w-full p-3 rounded-[8px] border text-left flex items-center justify-between transition-all ${
                      selectedExistingId === ws.id
                        ? 'bg-[#6B8C42]/10 border-[#8AB55A] text-[#E8D9A8]'
                        : 'bg-[#050505] border-[#23221C] text-[#8A8577] hover:border-[#8AB55A]/40'
                    }`}
                  >
                    <div>
                      <div className="font-bold text-xs text-[#E8D9A8] font-mono">{ws.name}</div>
                      <div className="text-[10px] text-[#8A8577] font-mono">
                        Environment:{' '}
                        <span className="uppercase text-[#8AB55A]">{ws.environment}</span> • Region:{' '}
                        {ws.region}
                      </div>
                    </div>
                    {selectedExistingId === ws.id && (
                      <CheckCircle2 className="w-4 h-4 text-[#8AB55A]" />
                    )}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Form Actions */}
          <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
            <button
              type="button"
              onClick={() => navigateToRoute('auth')}
              className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
            >
              <ArrowLeft className="w-3.5 h-3.5" /> Back
            </button>

            <button
              id="submit-workspace-btn"
              type="submit"
              disabled={isSubmitting || (mode === 'create' && !name.trim())}
              className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors disabled:opacity-40 shadow-sm"
            >
              <span>{mode === 'create' ? 'Create workspace' : 'Attach workspace'}</span>
              <ArrowRight className="w-3.5 h-3.5" />
            </button>
          </div>
        </form>
      </div>
    </div>
  );
};
