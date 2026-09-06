import React from 'react';

export interface TourStep {
  id: string;
  title: string;
  description: string[];
  target?: string;
}

export const TOUR_STEPS: TourStep[] = [
  {
    id: 'open',
    title: 'Open a Browser Instance',
    description: [
      'Each session is an autonomous browser instance with its own tab registry and cookies.',
      'Use the Session Deck on the left or select a verified workflow preset.',
      'Try starting with https://en.wikipedia.org or https://example.com.',
    ],
    target: '.session-manager',
  },
  {
    id: 'snapshot',
    title: 'Compact Accessibility Snapshot (<500 Tokens)',
    description: [
      'The snapshot maps all interactive elements to stable @eN references.',
      'Unlike fragile CSS selectors, @eN refs remain constant across inspection turns.',
      'Click any node in the list to stage it into the Action Deck.',
    ],
    target: '.snapshot-view',
  },
  {
    id: 'click',
    title: 'Trusted Mouse Click Dispatch',
    description: [
      'Select any element (e.g. @e1 "Learn more") and trigger Click.',
      'vakBrowse calculates the box-model center and dispatches genuine hardware mouse events.',
      'Automatic scrollIntoView({block:\'center\'}) brings below-the-fold elements into view.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'fill',
    title: 'Native Form & Text Input',
    description: [
      'Focuses the target element and applies native prototype property setters.',
      'Triggers synthetic input and change events — 100% React and Vue safe.',
      'Follow up with a keyboard "Enter" action to submit search boxes.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'extract',
    title: 'Readability-Style Markdown Extraction',
    description: [
      'Extracts clean main-content text without navigation boilerplate, headers, or ads.',
      'Compresses 500KB of article HTML down to ~20KB of high-density markdown for RAG.',
    ],
    target: '.result-pane',
  },
  {
    id: 'wire-protocol',
    title: 'Live Wire Protocol Inspector',
    description: [
      'Toggle "WIRE PROTOCOL JSON" in the right telemetry monitor to see the raw request/response.',
      'All surfaces (Daemon, CLI, MCP, Python SDK, and REST) speak this exact unified model.',
    ],
    target: '.result-pane',
  },
  {
    id: 'stealth',
    title: 'Hardware Stealth & Anti-Wall Evasions',
    description: [
      'Drops --enable-automation flags and masks navigator.webdriver.',
      'Injects genuine hardware plugin arrays and human-like Bézier pointer trajectories.',
    ],
    target: '.session-manager',
  },
  {
    id: 'dom-backend',
    title: 'Zero-Chromium DOM Backend (QuickJS)',
    description: [
      'Select "DOM" backend to run tests with zero Chromium processes.',
      'In-process HTML tokenization and embedded QuickJS for instant hermetic CI testing.',
    ],
    target: '.session-manager',
  },
];

interface Props {
  visible: boolean;
  onClose: () => void;
}

export function TourGuide({ visible, onClose }: Props) {
  if (!visible) return null;

  return (
    <div className="fixed inset-0 bg-black/80 backdrop-blur-xs flex items-center justify-center z-50 p-4 font-mono text-xs">
      <div className="te-panel rounded-xs border-accent p-6 w-full max-w-2xl max-h-[85vh] flex flex-col shadow-2xl space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-border">
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-xs bg-accent" />
            <span className="font-bold text-bone uppercase tracking-wider text-sm">
              VAKBROWSE CAPABILITY TOUR // SPEC MANUAL
            </span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-text text-sm font-bold"
          >
            ✕
          </button>
        </div>

        <div className="flex-1 overflow-y-auto space-y-4 pr-1">
          {TOUR_STEPS.map((step, i) => (
            <div key={step.id} className="p-3 bg-surface/50 border border-border rounded-xs space-y-1.5">
              <div className="flex items-center justify-between">
                <span className="font-bold text-accent">
                  [{String(i + 1).padStart(2, '0')}] {step.title}
                </span>
              </div>
              <ul className="space-y-1 text-text-dim text-[11px] font-sans">
                {step.description.map((desc, j) => (
                  <li key={j} className="flex items-start gap-1.5">
                    <span className="text-text-muted font-mono">›</span>
                    <span>{desc}</span>
                  </li>
                ))}
              </ul>
            </div>
          ))}
        </div>

        <div className="pt-3 border-t border-border flex justify-end">
          <button
            onClick={onClose}
            className="btn-primary"
          >
            EXIT TOUR GUIDE ›
          </button>
        </div>
      </div>
    </div>
  );
}
