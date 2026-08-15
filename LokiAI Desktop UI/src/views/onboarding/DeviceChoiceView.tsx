import React, { useState } from 'react';
import { useLoki } from '../../context/LokiContext';
import { OnboardingStepper } from '../../components/onboarding/OnboardingStepper';
import { Cpu, Hammer, ArrowRight, ArrowLeft } from 'lucide-react';

export const DeviceChoiceView: React.FC = () => {
  const { onboardingState, updateOnboardingState, navigateToRoute } = useLoki();
  const [selectedChoice, setSelectedChoice] = useState<'existing' | 'scratch'>(
    onboardingState.deviceChoice || 'existing',
  );

  const handleContinue = () => {
    updateOnboardingState({ deviceChoice: selectedChoice });
    if (selectedChoice === 'existing') {
      navigateToRoute('onboarding/device-connect');
    } else {
      navigateToRoute('onboarding/project');
    }
  };

  return (
    <div
      id="device-choice-screen"
      className="w-full max-w-3xl mx-auto py-4 px-4 font-sans animate-in fade-in duration-200"
    >
      <OnboardingStepper currentStep="device" />

      <div className="p-8 rounded-[12px] bg-[#0A0A08] border border-[#23221C] shadow-2xl space-y-6">
        {/* Header */}
        <div>
          <div className="flex items-center gap-2 text-[11px] font-mono text-[#8AB55A] uppercase tracking-wider mb-1">
            <Cpu className="w-3.5 h-3.5" /> Step 4: Hardware Setup Decision
          </div>
          <h1 className="text-xl font-bold text-[#E8D9A8]">Where are you starting?</h1>
          <p className="text-xs text-[#8A8577] mt-1.5 leading-relaxed">
            Choose whether to discover and validate connected physical hardware now, or initialize
            your workspace to start coding firmware immediately.
          </p>
        </div>

        {/* 2 Equal-Priority Cards */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {/* Option A: I already have a device */}
          <button
            id="choice-have-device"
            type="button"
            onClick={() => setSelectedChoice('existing')}
            className={`p-6 rounded-[12px] border text-left flex flex-col justify-between space-y-4 transition-all group ${
              selectedChoice === 'existing'
                ? 'bg-[#6B8C42]/10 border-[#8AB55A] shadow-md ring-1 ring-[#8AB55A]'
                : 'bg-[#050505] border-[#23221C] hover:border-[#8AB55A]/50'
            }`}
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="w-10 h-10 rounded-[8px] bg-[#23221C] border border-[#8AB55A]/30 flex items-center justify-center text-[#8AB55A]">
                  <Cpu className="w-5 h-5" />
                </div>
                {selectedChoice === 'existing' && (
                  <span className="w-6 h-6 rounded-full bg-[#8AB55A] text-[#010101] flex items-center justify-center text-xs font-bold">
                    ✓
                  </span>
                )}
              </div>

              <div>
                <h3 className="text-base font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                  I already have a device
                </h3>
                <p className="text-xs text-[#8A8577] mt-1 leading-relaxed">
                  Discover and connect hardware that is currently attached to or reachable from this
                  runtime daemon via USB, Serial, BLE, or Ethernet.
                </p>
              </div>
            </div>

            <div className="pt-3 border-t border-[#23221C]/60 text-[11px] font-mono text-[#8A8577]">
              Includes automatic 7-phase hardware health & telemetry validation.
            </div>
          </button>

          {/* Option B: I’m building from scratch */}
          <button
            id="choice-from-scratch"
            type="button"
            onClick={() => setSelectedChoice('scratch')}
            className={`p-6 rounded-[12px] border text-left flex flex-col justify-between space-y-4 transition-all group ${
              selectedChoice === 'scratch'
                ? 'bg-[#6B8C42]/10 border-[#8AB55A] shadow-md ring-1 ring-[#8AB55A]'
                : 'bg-[#050505] border-[#23221C] hover:border-[#8AB55A]/50'
            }`}
          >
            <div className="space-y-3">
              <div className="flex items-center justify-between">
                <div className="w-10 h-10 rounded-[8px] bg-[#23221C] border border-[#8AB55A]/30 flex items-center justify-center text-[#8AB55A]">
                  <Hammer className="w-5 h-5" />
                </div>
                {selectedChoice === 'scratch' && (
                  <span className="w-6 h-6 rounded-full bg-[#8AB55A] text-[#010101] flex items-center justify-center text-xs font-bold">
                    ✓
                  </span>
                )}
              </div>

              <div>
                <h3 className="text-base font-bold text-[#E8D9A8] group-hover:text-[#8AB55A] transition-colors">
                  I’m building from scratch
                </h3>
                <p className="text-xs text-[#8A8577] mt-1 leading-relaxed">
                  Create the workspace now and connect hardware when your project is ready. You can
                  test builds and diagnostic agents immediately.
                </p>
              </div>
            </div>

            <div className="pt-3 border-t border-[#23221C]/60 text-[11px] font-mono text-[#8A8577]">
              Full-featured workspace with live simulation and deferred hardware linking.
            </div>
          </button>
        </div>

        {/* Footer Navigation */}
        <div className="pt-4 border-t border-[#23221C] flex items-center justify-between">
          <button
            type="button"
            onClick={() => navigateToRoute('onboarding/agent')}
            className="flex items-center gap-1.5 px-4 py-2 rounded-[8px] bg-[#050505] border border-[#23221C] hover:bg-[#23221C] text-[#E8D9A8] text-xs font-semibold transition-colors"
          >
            <ArrowLeft className="w-3.5 h-3.5" /> Back
          </button>

          <button
            id="device-choice-continue-btn"
            type="button"
            onClick={handleContinue}
            className="flex items-center gap-2 px-5 py-2 rounded-[8px] bg-[#8AB55A] text-[#010101] hover:bg-[#A8C878] text-xs font-bold transition-colors shadow-sm"
          >
            <span>Continue</span>
            <ArrowRight className="w-3.5 h-3.5" />
          </button>
        </div>
      </div>
    </div>
  );
};
