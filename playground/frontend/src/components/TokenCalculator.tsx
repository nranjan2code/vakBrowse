import React, { useState } from 'react';

export function TokenCalculator() {
  const [pagesPerDay, setPagesPerDay] = useState<number>(500);
  const [model, setModel] = useState<'claude4' | 'gpt5' | 'gemini25'>('claude4');

  // Input token pricing per 1M tokens ($) — 2026 frontier inference rates
  const modelPricing = {
    claude4: { name: 'Claude 4 Sonnet', costPerMillion: 3.0 },
    gpt5: { name: 'GPT-5 / o3', costPerMillion: 2.5 },
    gemini25: { name: 'Gemini 2.5 Pro', costPerMillion: 1.5 },
  };

  const currentPricing = modelPricing[model];

  // Tokens per page
  const rawTokensPerPage = 45000;
  const playwrightTokensPerPage = 12000;
  const vakTokensPerPage = 420;

  // Monthly calculations (30 days)
  const monthlyPages = pagesPerDay * 30;

  const rawMonthlyTokens = monthlyPages * rawTokensPerPage;
  const playwrightMonthlyTokens = monthlyPages * playwrightTokensPerPage;
  const vakMonthlyTokens = monthlyPages * vakTokensPerPage;

  const rawCost = (rawMonthlyTokens / 1_000_000) * currentPricing.costPerMillion;
  const playwrightCost = (playwrightMonthlyTokens / 1_000_000) * currentPricing.costPerMillion;
  const vakCost = (vakMonthlyTokens / 1_000_000) * currentPricing.costPerMillion;

  const monthlySavingsVsRaw = rawCost - vakCost;
  const percentageSavings = (((rawCost - vakCost) / rawCost) * 100).toFixed(1);

  return (
    <div className="te-panel p-6 border-border rounded-xs max-w-4xl mx-auto">
      {/* Panel header with industrial labels */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between pb-4 border-b border-border gap-2">
        <div className="flex items-center gap-2">
          <div className="w-2.5 h-2.5 bg-yellow rounded-full animate-pulse shadow-[0_0_8px_rgba(255,184,0,0.6)]" />
          <span className="font-mono font-bold text-sm text-bone uppercase tracking-wider">
            TOKEN ECONOMICS ANALYZER // VAK-CALC-400
          </span>
        </div>
        <div className="badge-orange">
          SAVINGS EFFICIENCY: {percentageSavings}%
        </div>
      </div>

      {/* Controls row */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6 my-6">
        {/* Slider: Actions / Pages per day */}
        <div className="space-y-2">
          <div className="flex justify-between font-mono text-xs">
            <span className="text-text-dim uppercase tracking-wider">PAGES INSPECTED / DAY:</span>
            <span className="text-accent font-bold text-sm">{pagesPerDay.toLocaleString()}</span>
          </div>
          <input
            type="range"
            min="50"
            max="10000"
            step="50"
            value={pagesPerDay}
            onChange={(e) => setPagesPerDay(Number(e.target.value))}
            className="w-full accent-accent bg-surface h-2 rounded-xs cursor-pointer"
          />
          <div className="flex justify-between text-[10px] font-mono text-text-muted">
            <span>50 (HOBBY)</span>
            <span>1,000 (PRODUCTION)</span>
            <span>10,000 (FLEET)</span>
          </div>
        </div>

        {/* Model Selector */}
        <div className="space-y-2">
          <div className="font-mono text-xs text-text-dim uppercase tracking-wider">
            TARGET INFERENCE MODEL:
          </div>
          <div className="grid grid-cols-3 gap-2">
            {(['claude4', 'gpt5', 'gemini25'] as const).map((m) => (
              <button
                key={m}
                onClick={() => setModel(m)}
                className={`py-2 px-1 text-xs font-mono rounded-xs border transition-all ${
                  model === m
                    ? 'bg-accent/15 border-accent text-bone font-semibold shadow-xs'
                    : 'bg-surface border-border text-text-dim hover:text-text'
                }`}
              >
                {m === 'claude4' ? 'Claude 4 Sonnet' : m === 'gpt5' ? 'GPT-5 / o3' : 'Gemini 2.5 Pro'}
              </button>
            ))}
          </div>
        </div>
      </div>

      {/* Comparison Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-4 my-6">
        {/* Card 1: Raw DOM */}
        <div className="p-4 bg-surface/50 border border-border rounded-xs space-y-3">
          <div className="text-[11px] font-mono text-text-muted uppercase">01 // RAW DOM DUMP</div>
          <div className="text-2xl font-mono font-bold text-text-dim">
            ${rawCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            <span className="text-xs font-normal text-text-muted">/mo</span>
          </div>
          <div className="space-y-1 text-xs font-mono text-text-muted">
            <div className="flex justify-between">
              <span>Tokens/Page:</span>
              <span className="text-text-dim">~45,000</span>
            </div>
            <div className="flex justify-between">
              <span>Context Drain:</span>
              <span className="text-danger font-semibold">HIGH</span>
            </div>
          </div>
        </div>

        {/* Card 2: Playwright A11y */}
        <div className="p-4 bg-surface/50 border border-border rounded-xs space-y-3">
          <div className="text-[11px] font-mono text-text-muted uppercase">02 // PLAYWRIGHT A11Y</div>
          <div className="text-2xl font-mono font-bold text-text-dim">
            ${playwrightCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            <span className="text-xs font-normal text-text-muted">/mo</span>
          </div>
          <div className="space-y-1 text-xs font-mono text-text-muted">
            <div className="flex justify-between">
              <span>Tokens/Page:</span>
              <span className="text-text-dim">~12,000</span>
            </div>
            <div className="flex justify-between">
              <span>Context Drain:</span>
              <span className="text-yellow font-semibold">MEDIUM</span>
            </div>
          </div>
        </div>

        {/* Card 3: vakBrowse */}
        <div className="p-4 bg-card border-2 border-accent rounded-xs space-y-3 shadow-te-button relative overflow-hidden">
          <div className="absolute top-0 right-0 bg-accent text-white font-mono text-[9px] px-2 py-0.5 uppercase tracking-widest font-bold">
            VAKBROWSE
          </div>
          <div className="text-[11px] font-mono text-accent uppercase font-bold">
            03 // VAKBROWSE PERCEPTION
          </div>
          <div className="text-2xl font-mono font-bold text-bone">
            ${vakCost.toLocaleString('en-US', { maximumFractionDigits: 0 })}
            <span className="text-xs font-normal text-accent font-mono ml-1">/mo</span>
          </div>
          <div className="space-y-1 text-xs font-mono">
            <div className="flex justify-between text-text-dim">
              <span>Tokens/Page:</span>
              <span className="text-yellow font-semibold">&lt;420 TOKENS</span>
            </div>
            <div className="flex justify-between text-text-dim">
              <span>Context Drain:</span>
              <span className="text-emerald-400 font-semibold">MINIMAL (-99%)</span>
            </div>
          </div>
        </div>
      </div>

      {/* Bottom Summary Banner */}
      <div className="p-3 bg-surface border border-border rounded-xs flex flex-col sm:flex-row items-center justify-between font-mono text-xs gap-2">
        <div className="flex items-center gap-2">
          <span className="text-accent font-bold">ESTIMATED ANNUAL SAVINGS:</span>
          <span className="text-bone font-bold text-sm">
            ${(monthlySavingsVsRaw * 12).toLocaleString('en-US', { maximumFractionDigits: 0 })}
          </span>
        </div>
        <div className="text-[11px] text-text-dim">
          <span>{percentageSavings}% TOKEN EXPENSE REDUCTION</span>
        </div>
      </div>
    </div>
  );
}
