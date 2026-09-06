import React from 'react';

// Guided tour that walks through all vakBrowse capabilities.
// Each step highlights a UI region and explains what to look for.

export interface TourStep {
  id: string;
  title: string;
  description: string[];
  target?: string; // CSS selector of element to highlight
  action?: () => Promise<void>;
}

export const TOUR_STEPS: TourStep[] = [
  {
    id: 'open',
    title: 'Open a Session',
    description: [
      'Sessions are the unit of browser interaction. Each session is a Chrome tab.',
      'Use the Session Manager on the left to open a new browser session.',
      'Try opening https://example.com to start.',
    ],
    target: '.session-manager',
  },
  {
    id: 'snapshot',
    title: 'Accessibility Snapshot',
    description: [
      'The snapshot shows all interactive elements on the page as @eN refs.',
      'These refs are stable across snapshots and are how you target elements for click/fill.',
      'Click any element in the snapshot to select it.',
    ],
    target: '.snapshot-view',
  },
  {
    id: 'click',
    title: 'Click an Element',
    description: [
      'Select an element (e.g. @e1 "Learn more" on example.com) and click it.',
      'The browser will click the element using Input.dispatchMouseEvent at its center.',
      'If the click causes navigation, the snapshot auto-refreshes to the new page.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'fill',
    title: 'Fill Text',
    description: [
      'For input/textarea elements, use the Fill field to set text.',
      'vakBrowse focuses the element first, then sets its value with input/change events — React/Vue-safe.',
      'On Wikipedia, fill the search box and press Enter to search.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'find',
    title: 'Find by CSS',
    description: [
      'CSS selectors resolve to @eN refs immediately — no intermediate snapshot needed.',
      'On CDP, the selector runs via document.querySelectorAll and matches back to snapshot refs.',
      'Try "a[href*=\"privacy"]" on an iana.org page.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'extract',
    title: 'Extract Content',
    description: [
      'The Extract button returns readability-style main-content text.',
      'It strips navigation, ads, and boilerplate — perfect for reading articles.',
      'Token-efficient: ~20KB from a 500KB Wikipedia page.',
    ],
    target: '.result-pane',
  },
  {
    id: 'source',
    title: 'Page Source',
    description: [
      'Returns the full HTML source of the current page.',
      'Useful for understanding page structure before picking selectors.',
    ],
    target: '.result-pane',
  },
  {
    id: 'eval',
    title: 'JavaScript Evaluation',
    description: [
      'Eval any JavaScript expression. Returns are stringified (REPL semantics).',
      'Try "navigator.webdriver" — false under stealth, true otherwise.',
      'Objects/arrays become JSON strings; numbers/booleans stringify.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'screenshot',
    title: 'Screenshot',
    description: [
      'Takes a real PNG screenshot of the page.',
      'Base64-encoded for easy embedding in results.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'file-upload',
    title: 'File Upload',
    description: [
      'Set file input elements with the File Upload field.',
      'Uses DOM.setFileInputFiles to bypass browser security on programmatic .files assignment.',
      'Enter paths as comma-separated: /path/to/file1, /path/to/file2',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'cookies',
    title: 'Cookies & Downloads',
    description: [
      'List, set, and clear cookies per session.',
      'Download listing reads the configured download directory.',
      'Set download directory before downloading files.',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'batch',
    title: 'Action Batching',
    description: [
      'Batch multiple actions in one request — fail-fast on first error.',
      'Each action returns a result, giving you one result per action.',
      'Try: [{"type":"navigate","url":"https://example.com"},{"type":"extract"}]',
    ],
    target: '.action-toolbar',
  },
  {
    id: 'stealth',
    title: 'Stealth Mode',
    description: [
      'When opening a session, enable Stealth to defeat webdriver/plugin/pointer tells.',
      'It does NOT defeat TLS fingerprinting or behavioral biometrics.',
      'Site isolation is intentionally left ON.',
    ],
    target: '.session-manager',
  },
  {
    id: 'proxy',
    title: 'Proxy & Rotation',
    description: [
      'Set a single proxy via --proxy, or a pool via --proxies a,b.',
      'RotateProxy re-launches Chrome on the next endpoint and restores the session URL.',
      'Rotation changes source IP only — does not defeat TLS/HTTP2 fingerprinting.',
    ],
    target: '.session-manager',
  },
  {
    id: 'dom-backend',
    title: 'DOM Backend',
    description: [
      'Select "DOM" backend to use a pure-Rust HTML parser + QuickJS — no Chrome process.',
      'File-only navigation. JS eval and event dispatch work in-process.',
      'Proves the engine seam is swappable.',
    ],
    target: '.session-manager',
  },
  {
    id: 'done',
    title: 'You\'re Ready!',
    description: [
      'vakBrowse gives AI agents a real, scriptable web browser.',
      'All capabilities go through the EngineLauncher/PageOps traits.',
      'The daemon, CLI, MCP, REST API, and FFI all speak the same Request model.',
      '',
      'Happy browsing!',
    ],
  },
];

interface Props {
  visible: boolean;
  onClose: () => void;
}

export function TourGuide({ visible, onClose }: Props) {
  if (!visible) return null;

  return (
    <div className="fixed inset-0 bg-black/60 flex items-center justify-center z-50 p-4">
      <div className="bg-card border border-border rounded-lg p-6 w-full max-w-2xl max-h-[80vh] overflow-y-auto">
        <h2 className="text-xl font-medium text-text mb-4">vakBrowse Playground Tour</h2>
        <div className="space-y-3 text-sm">
          {TOUR_STEPS.map((step, i) => (
            <div key={step.id} className="border-b border-border pb-3 last:border-0">
              <h3 className="font-medium text-accent">{i + 1}. {step.title}</h3>
              <ul className="mt-1 space-y-1 text-text-dim">
                {step.description.map((desc, j) => (
                  desc ? <li key={j}>• {desc}</li> : <li key={j}>&nbsp;</li>
                ))}
              </ul>
            </div>
          ))}
        </div>
        <button
          onClick={onClose}
          className="mt-4 px-4 py-2 bg-accent hover:bg-accent-hover text-white rounded"
        >
          Close Tour
        </button>
      </div>
    </div>
  );
}
