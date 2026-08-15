import React from 'react';
import { OnboardingStepName } from '../../types';
import { Check, Shield, Briefcase, Bot, Cpu, Sparkles } from 'lucide-react';

interface OnboardingStepperProps {
  currentStep: OnboardingStepName;
  onStepClick?: (step: OnboardingStepName) => void;
}

interface StepConfig {
  id: OnboardingStepName;
  label: string;
  sublabel: string;
  icon: any;
}

const steps: StepConfig[] = [
  { id: 'account', label: 'Account', sublabel: 'LokiAI Auth', icon: Shield },
  { id: 'workspace', label: 'Workspace', sublabel: 'Environment', icon: Briefcase },
  { id: 'agent', label: 'Coding agent', sublabel: 'AI Bridge', icon: Bot },
  { id: 'device', label: 'Device', sublabel: 'Hardware', icon: Cpu },
  { id: 'ready', label: 'Ready', sublabel: 'Verification', icon: Sparkles },
];

export const OnboardingStepper: React.FC<OnboardingStepperProps> = ({ currentStep }) => {
  const stepIndexMap: Record<OnboardingStepName, number> = {
    account: 0,
    workspace: 1,
    agent: 2,
    device: 3,
    ready: 4,
  };

  const currentIndex = stepIndexMap[currentStep];

  return (
    <nav
      id="onboarding-stepper"
      aria-label="Onboarding Progress"
      className="w-full max-w-4xl mx-auto mb-8 px-4"
    >
      <ol className="flex items-center justify-between relative">
        {/* Background Connecting Line */}
        <div
          className="absolute left-6 right-6 top-1/2 -translate-y-1/2 h-[1px] bg-[#23221C] -z-0"
          aria-hidden="true"
        />

        {steps.map((step, idx) => {
          const isCompleted = idx < currentIndex;
          const isCurrent = idx === currentIndex;
          const Icon = step.icon;

          return (
            <li
              key={step.id}
              className="relative z-10 flex flex-col items-center group cursor-default"
            >
              <div
                className={`w-9 h-9 rounded-full flex items-center justify-center text-xs font-mono transition-all duration-200 ${
                  isCompleted
                    ? 'bg-[#6B8C42]/20 border border-[#8AB55A] text-[#8AB55A]'
                    : isCurrent
                      ? 'bg-[#0A0A08] border-2 border-[#8AB55A] text-[#E8D9A8] shadow-[0_0_12px_rgba(138,181,90,0.15)]'
                      : 'bg-[#0A0A08] border border-[#23221C] text-[#8A8577]'
                }`}
              >
                {isCompleted ? (
                  <Check className="w-4 h-4 stroke-[2.5]" />
                ) : (
                  <Icon className={`w-4 h-4 ${isCurrent ? 'text-[#8AB55A]' : 'text-[#8A8577]'}`} />
                )}
              </div>

              <div className="mt-2 text-center">
                <span
                  className={`block text-xs font-semibold font-sans ${
                    isCurrent ? 'text-[#E8D9A8]' : isCompleted ? 'text-[#8AB55A]' : 'text-[#8A8577]'
                  }`}
                >
                  {step.label}
                </span>
                <span className="hidden sm:block text-[10px] font-mono text-[#8A8577]/80">
                  {step.sublabel}
                </span>
              </div>
            </li>
          );
        })}
      </ol>
    </nav>
  );
};
