import React from 'react';
import { TokenCalculator } from '../components/TokenCalculator';
import type { PageTab } from '../components/Header';
import tokenUnitImg from '../assets/token_unit.jpg';

interface EconomicsPageProps {
  onNavigate: (tab: PageTab) => void;
}

export function EconomicsPage({ onNavigate }: EconomicsPageProps) {
  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-10 space-y-14">
      {/* Header */}
      <div className="space-y-3">
        <div className="badge-yellow">FINANCIAL ENGINEERING // ROI MODEL</div>
        <h1 className="text-3xl sm:text-4xl font-bold font-sans text-bone">
          Token Economics & Context Budget Optimization
        </h1>
        <p className="text-sm sm:text-base text-text-dim font-sans max-w-3xl">
          In LLM agent architectures, the browser is the single largest consumer of context window tokens.
          Slashing input tokens by 98% directly cuts inferencing bills and preserves vital context space for agent reasoning.
        </p>
      </div>

      {/* Hardware Instrument Banner */}
      <div className="te-panel rounded-xs border-border p-6 space-y-4">
        <div className="flex items-center justify-between pb-3 border-b border-border">
          <span className="font-mono text-xs text-bone font-bold uppercase tracking-wider">
            PERCEPTION SENSOR // MODEL VAK-AX
          </span>
          <span className="badge-orange">420 TOKENS / PAGE BENCHMARK</span>
        </div>

        <div className="relative aspect-[16/9] bg-surface rounded-xs overflow-hidden border border-border">
          <img
            src={tokenUnitImg}
            alt="vakBrowse Perception Sensor Module VAK-AX"
            className="w-full h-full object-cover"
          />
        </div>
        
        <div className="text-[11px] font-mono text-text-muted flex justify-between">
          <span>CALIBRATION: BENCHMARKED ACROSS 1,000 PRODUCTION PAGES</span>
          <span>COMPRESSION FACTOR: 98.6%</span>
        </div>
      </div>

      {/* The Math Behind the Savings */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 font-mono text-xs">
        <div className="te-panel rounded-xs border-border p-5 space-y-3">
          <div className="text-danger font-bold uppercase">[A] RAW HTML DUMP</div>
          <div className="text-3xl font-bold text-bone">~45,000</div>
          <div className="text-text-muted text-[11px]">AVG TOKENS PER PAGE</div>
          <p className="text-text-dim font-sans text-xs leading-relaxed pt-2 border-t border-border">
            Dumps tracking pixels, SVG geometry, obfuscated CSS classes, analytics bundles, and DOM wrappers that offer zero actionability.
          </p>
        </div>

        <div className="te-panel rounded-xs border-border p-5 space-y-3">
          <div className="text-yellow font-bold uppercase">[B] PLAYWRIGHT ACCESSIBILITY</div>
          <div className="text-3xl font-bold text-bone">~12,000</div>
          <div className="text-text-muted text-[11px]">AVG TOKENS PER PAGE</div>
          <p className="text-text-dim font-sans text-xs leading-relaxed pt-2 border-t border-border">
            Includes hundreds of nested generic containers, layout groupings, and non-interactive text fragments that drain token limits.
          </p>
        </div>

        <div className="te-panel rounded-xs border-2 border-accent p-5 space-y-3">
          <div className="text-accent font-bold uppercase">[C] VAKBROWSE PERCEPTION</div>
          <div className="text-3xl font-bold text-accent">~420</div>
          <div className="text-text-muted text-[11px]">AVG TOKENS PER PAGE</div>
          <p className="text-text-dim font-sans text-xs leading-relaxed pt-2 border-t border-border">
            Pure actionable affordance graph: only interactive elements with stable <code className="text-accent">@eN</code> IDs, clean roles, and current values.
          </p>
        </div>
      </div>

      {/* Interactive Calculator Section */}
      <div className="space-y-4">
        <div className="text-center max-w-xl mx-auto space-y-1">
          <h3 className="text-xl font-bold font-sans text-bone">
            Fleet Token Cost Simulator
          </h3>
          <p className="text-xs text-text-dim font-sans">
            Adjust volume and target LLM model to model your organization's monthly token budget.
          </p>
        </div>
        <TokenCalculator />
      </div>

      {/* Enterprise Impact List */}
      <div className="te-panel rounded-xs border-border p-6 space-y-4">
        <h4 className="text-base font-bold font-sans text-bone">
          Direct Operational Benefits
        </h4>
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4 text-xs font-mono">
          <div className="flex items-start gap-2.5">
            <span className="text-accent font-bold">01</span>
            <div className="space-y-0.5">
              <span className="text-bone font-semibold">10x More Actions Per Conversation Turn:</span>
              <p className="text-text-dim font-sans">
                Agents can browse 25 sequential pages without exceeding context windows or needing memory-loss truncations.
              </p>
            </div>
          </div>

          <div className="flex items-start gap-2.5">
            <span className="text-accent font-bold">02</span>
            <div className="space-y-0.5">
              <span className="text-bone font-semibold">Faster Reasoning Latency:</span>
              <p className="text-text-dim font-sans">
                Prompt evaluation time for 400 tokens is ~35ms, compared to 1,200ms+ for a 50k token raw DOM document.
              </p>
            </div>
          </div>

          <div className="flex items-start gap-2.5">
            <span className="text-accent font-bold">03</span>
            <div className="space-y-0.5">
              <span className="text-bone font-semibold">Zero Selector Fragility:</span>
              <p className="text-text-dim font-sans">
                Models select elements by simple references (<code className="text-accent">@e1</code>) or semantic descriptions rather than fragile CSS class selectors.
              </p>
            </div>
          </div>

          <div className="flex items-start gap-2.5">
            <span className="text-accent font-bold">04</span>
            <div className="space-y-0.5">
              <span className="text-bone font-semibold">Failsafe Navigation Detection:</span>
              <p className="text-text-dim font-sans">
                Self-healing snapshots reconcile against the live browser URL automatically, updating references without manual refreshes.
              </p>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
