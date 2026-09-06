import React, { useState } from 'react';

export interface PresetScenario {
  id: string;
  name: string;
  category: 'RESEARCH' | 'EXTRACTION' | 'FORMS' | 'STEALTH' | 'BATCH';
  description: string;
  initialUrl: string;
  stealth: boolean;
  notes: string;
  actionSequence: {
    actionType: string;
    details: string;
  }[];
}

export const PRESET_SCENARIOS: PresetScenario[] = [
  {
    id: 'wikipedia-research',
    name: 'Wikipedia Research',
    category: 'RESEARCH',
    description: 'Loads live encyclopedia knowledge, captures a <400 token perception graph, and performs readability extraction.',
    initialUrl: 'https://en.wikipedia.org/wiki/Artificial_intelligence',
    stealth: false,
    notes: 'Verified safe: Wikipedia never blocks or CAPTCHAs synthetic browsers.',
    actionSequence: [
      { actionType: '1. Navigate', details: 'Load article URL' },
      { actionType: '2. Snapshot', details: 'Parse <400 token accessibility graph' },
      { actionType: '3. Extract', details: 'Extract readable clean markdown' },
    ],
  },
  {
    id: 'hn-intelligence',
    name: 'Hacker News Digest',
    category: 'EXTRACTION',
    description: 'Inspects developer discussions, maps stable @eN references, and extracts top story threads.',
    initialUrl: 'https://news.ycombinator.com',
    stealth: false,
    notes: 'Verified safe: Minimal HTML, instant response, zero bot walls.',
    actionSequence: [
      { actionType: '1. Navigate', details: 'news.ycombinator.com' },
      { actionType: '2. Snapshot', details: 'Map links & vote arrows to @eN refs' },
      { actionType: '3. Extract', details: 'Extract frontpage discussion digest' },
    ],
  },
  {
    id: 'example-link-proof',
    name: 'IANA Link Proof',
    category: 'RESEARCH',
    description: 'Navigates to example.com and clicks "Learn more" (@e1) to verify trusted mouse center dispatch and URL change.',
    initialUrl: 'https://example.com',
    stealth: false,
    notes: 'Verified safe: Canonical dogfood test from vakBrowse test suite.',
    actionSequence: [
      { actionType: '1. Navigate', details: 'https://example.com' },
      { actionType: '2. Snapshot', details: 'Find @e1 ("Learn more")' },
      { actionType: '3. Click @e1', details: 'Navigates to www.iana.org' },
    ],
  },
  {
    id: 'wikipedia-form-search',
    name: 'Search & Submit',
    category: 'FORMS',
    description: 'Tests native input value setter on Wikipedia search, then dispatches trusted Enter key to navigate.',
    initialUrl: 'https://en.wikipedia.org/wiki/Special:Search',
    stealth: false,
    notes: 'Verified safe: Clean form automation without Cloudflare or CAPTCHAs.',
    actionSequence: [
      { actionType: '1. Navigate', details: 'Special:Search' },
      { actionType: '2. Fill', details: 'Set query to "Rust programming language"' },
      { actionType: '3. PressKey', details: 'Dispatch "Enter" to trigger submit' },
    ],
  },
  {
    id: 'stealth-probe',
    name: 'Stealth Audit',
    category: 'STEALTH',
    description: 'Probes navigator.webdriver, plugin presence, and language arrays to verify automated flags are absent.',
    initialUrl: 'https://example.com',
    stealth: true,
    notes: 'Verifies navigator.webdriver evaluates to false.',
    actionSequence: [
      { actionType: '1. Open', details: 'Launch with --stealth profile' },
      { actionType: '2. Eval JS', details: 'navigator.webdriver -> expect false' },
      { actionType: '3. Eval JS', details: 'navigator.languages -> expect ["en-US"]' },
    ],
  },
  {
    id: 'batch-pipeline',
    name: 'Batch Pipeline',
    category: 'BATCH',
    description: 'Runs multiple actions in a single atomic Request::Batch round-trip, reducing network latency.',
    initialUrl: 'https://example.com',
    stealth: false,
    notes: 'Verified safe: Single round-trip over UDS / HTTP.',
    actionSequence: [
      { actionType: '1. Batch', details: '[{"type":"extract"},{"type":"screenshot"}]' },
      { actionType: '2. Results', details: 'All results returned in one wire frame' },
    ],
  },
];

interface PresetWorkflowsProps {
  onSelectScenario: (scenario: PresetScenario) => void;
  activeScenarioId: string | null;
}

export function PresetWorkflows({ onSelectScenario, activeScenarioId }: PresetWorkflowsProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="te-panel rounded-xs border-border bg-card">
      {/* Compact single-row bar */}
      <div className="px-3 py-1.5 flex flex-wrap items-center justify-between gap-2 text-xs font-mono">
        <div className="flex items-center gap-2 flex-wrap">
          <span className="flex items-center gap-1.5 text-bone font-bold text-[11px]">
            <span className="w-2 h-2 rounded-xs bg-accent" />
            WORKFLOW PRESETS:
          </span>
          <div className="flex items-center gap-1.5 overflow-x-auto py-0.5">
            {PRESET_SCENARIOS.map((s) => {
              const isSelected = activeScenarioId === s.id;
              return (
                <button
                  key={s.id}
                  onClick={() => onSelectScenario(s)}
                  className={`px-2 py-0.5 rounded-xs text-[11px] font-mono tracking-wider transition-all flex items-center gap-1 whitespace-nowrap ${
                    isSelected
                      ? 'bg-accent text-white font-bold shadow-xs'
                      : 'bg-surface hover:bg-surface-elevated text-bone border border-border hover:border-border-strong'
                  }`}
                  title={`${s.description} (${s.initialUrl})`}
                >
                  <span className="text-[10px] text-accent">⚡</span>
                  <span>{s.name}</span>
                </button>
              );
            })}
          </div>
        </div>

        <button
          onClick={() => setExpanded(!expanded)}
          className="text-[10px] text-text-dim hover:text-text uppercase font-semibold flex items-center gap-1 transition-colors"
        >
          <span>{expanded ? '▲ HIDE DETAILS' : '▼ DETAILS'}</span>
        </button>
      </div>

      {/* Expanded detailed cards (shown only when user clicks DETAILS) */}
      {expanded && (
        <div className="p-3 border-t border-border grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-6 gap-2.5 bg-bg/80">
          {PRESET_SCENARIOS.map((scenario) => {
            const isSelected = activeScenarioId === scenario.id;
            return (
              <div
                key={scenario.id}
                onClick={() => onSelectScenario(scenario)}
                className={`p-2.5 rounded-xs border cursor-pointer transition-all flex flex-col justify-between text-left group ${
                  isSelected
                    ? 'bg-surface-elevated border-accent shadow-te-button'
                    : 'bg-card border-border hover:border-border-strong hover:bg-surface'
                }`}
              >
                <div>
                  <div className="flex items-center justify-between mb-1">
                    <span className="text-[9px] font-mono px-1 py-0.5 rounded-xs uppercase tracking-wider font-semibold bg-surface border border-border text-text-dim">
                      {scenario.category}
                    </span>
                    {scenario.stealth && (
                      <span className="text-[9px] font-mono text-yellow font-bold">
                        STEALTH
                      </span>
                    )}
                  </div>

                  <div className="font-mono font-bold text-xs text-bone group-hover:text-accent transition-colors leading-snug mb-1">
                    {scenario.name}
                  </div>

                  <div className="text-[10px] font-sans text-text-dim leading-relaxed line-clamp-2">
                    {scenario.description}
                  </div>
                </div>

                <div className="mt-2 pt-1.5 border-t border-border/70 flex items-center justify-between text-[10px] font-mono">
                  <span className="text-text-muted truncate max-w-[80px]">
                    {scenario.initialUrl.replace('https://', '')}
                  </span>
                  <span className="text-accent font-semibold flex items-center gap-0.5">
                    LAUNCH ›
                  </span>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
