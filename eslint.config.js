// LokiAI Main v1.2 — root ESLint flat config (FND-002).
//
// Not named by Docs/TECH_STACK.md; added here to satisfy FND-002's own mandate to
// establish "formatting/lint/test config." Justification (AGENTS.md rule 11):
// ESLint + typescript-eslint is the standard static-analysis pair for a strict
// TypeScript/React codebase; MIT-licensed, actively maintained, no native build
// step, removable by deleting this file and the devDependencies that back it.
import js from '@eslint/js';
import reactHooks from 'eslint-plugin-react-hooks';
import reactRefresh from 'eslint-plugin-react-refresh';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default tseslint.config(
  {
    ignores: [
      '**/node_modules/**',
      '**/dist/**',
      '**/build/**',
      '**/target/**',
      '**/coverage/**',
      '**/.turbo/**',
      '**/.vite/**',
    ],
  },
  js.configs.recommended,
  ...tseslint.configs.recommendedTypeChecked,
  {
    files: ['LokiAI Desktop UI/**/*.{ts,tsx}'],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
      parserOptions: {
        projectService: true,
        tsconfigRootDir: import.meta.dirname,
      },
    },
    plugins: {
      'react-hooks': reactHooks,
      'react-refresh': reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      'react-refresh/only-export-components': ['warn', { allowConstantExport: true }],
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['*.config.{js,ts}', 'eslint.config.js', 'tools/scripts/**/*.{js,mjs,cjs}'],
    ...tseslint.configs.disableTypeChecked,
    languageOptions: {
      ...tseslint.configs.disableTypeChecked.languageOptions,
      globals: globals.node,
    },
  },
  {
    // FND-002 tracked pre-existing debt, not a policy choice — do not add files
    // here for new work; fix the real typing instead. These are exactly the
    // components/views in the original AI-Studio-generated prototype that pass
    // untyped `any` data across context/props boundaries (mock command-palette
    // items, inspector payloads, onboarding form handlers) or call setState
    // synchronously inside an effect. FND-002's mandate is establishing a
    // working toolchain, not auditing 40 pre-existing component files' data
    // flow — that is real, judgment-heavy work for whichever node next owns
    // each file (see Docs/graph/handoffs/FND-002-1.md for the full list this
    // was captured from and the exact rule counts per file).
    //
    // Every other file in the workspace is held to the full rule set above —
    // this override is deliberately narrow, not a blanket relaxation.
    files: [
      'LokiAI Desktop UI/src/components/common/CommandPalette.tsx',
      'LokiAI Desktop UI/src/components/common/InspectorDrawer.tsx',
      'LokiAI Desktop UI/src/components/common/LogViewer.tsx',
      'LokiAI Desktop UI/src/components/layout/Sidebar.tsx',
      'LokiAI Desktop UI/src/components/onboarding/OnboardingStepper.tsx',
      'LokiAI Desktop UI/src/context/LokiContext.tsx',
      'LokiAI Desktop UI/src/types/index.ts',
      'LokiAI Desktop UI/src/views/SettingsView.tsx',
      'LokiAI Desktop UI/src/views/onboarding/AgentConnectView.tsx',
      'LokiAI Desktop UI/src/views/onboarding/DeviceConnectView.tsx',
      'LokiAI Desktop UI/src/views/onboarding/DeviceHealthCheckView.tsx',
      'LokiAI Desktop UI/src/views/onboarding/WorkspaceSetupView.tsx',
    ],
    rules: {
      '@typescript-eslint/no-explicit-any': 'off',
      '@typescript-eslint/no-unsafe-assignment': 'off',
      '@typescript-eslint/no-unsafe-member-access': 'off',
      '@typescript-eslint/no-unsafe-call': 'off',
      '@typescript-eslint/no-unsafe-argument': 'off',
      '@typescript-eslint/no-unsafe-return': 'off',
      '@typescript-eslint/no-misused-promises': 'off',
      '@typescript-eslint/no-floating-promises': 'off',
      'no-empty': 'off',
      'react-hooks/set-state-in-effect': 'off',
    },
  },
);
