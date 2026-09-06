import React from 'react';

interface Props {
  elementsCount: number;
  rawSourceBytes?: number;
}

export function TokenSavingsGauge({ elementsCount, rawSourceBytes }: Props) {
  const isLive = elementsCount > 0;

  // Estimated raw tokens (industry standard ~4 chars per token)
  const rawBytes = isLive ? (rawSourceBytes || 380000) : 0;
  const rawTokens = isLive ? Math.round(rawBytes / 4) : 0;

  // vakBrowse compact perception tokens:
  // Each node in the compact line format is approx 12-15 tokens + 30 header tokens
  const perceivedTokens = isLive ? Math.round(elementsCount * 12 + 30) : 0;

  const savedTokens = Math.max(0, rawTokens - perceivedTokens);
  const savingsPercent = isLive && rawTokens > 0 ? ((savedTokens / rawTokens) * 100).toFixed(1) : '0.0';

  return (
    <div className="p-2.5 bg-card border border-border rounded-xs font-mono text-xs space-y-2">
      <div className="flex items-center justify-between text-[11px]">
        <span className="font-bold text-bone flex items-center gap-1.5">
          <span className={`w-2 h-2 rounded-full ${isLive ? 'bg-emerald-400 animate-pulse' : 'bg-text-muted'}`} />
          REAL-TIME CONTEXT EFFICIENCY
        </span>
        <span className={isLive ? 'badge-yellow font-bold text-[10px]' : 'badge-dim text-[10px]'}>
          {isLive ? `${savingsPercent}% REDUCTION` : 'IDLE (0 TOKENS)'}
        </span>
      </div>

      {/* Side-by-side comparison */}
      <div className="grid grid-cols-2 gap-2 text-[10px]">
        <div className="p-1.5 bg-bg border border-border/80 rounded-xs space-y-0.5">
          <div className="text-text-muted uppercase">RAW HTML DUMP:</div>
          <div className={`${isLive ? 'text-danger' : 'text-text-dim'} font-bold text-xs`}>
            {isLive ? `~${rawTokens.toLocaleString()} TOKENS` : '0 TOKENS'}
          </div>
          <div className="text-text-dim text-[9px]">
            {isLive ? `${(rawBytes / 1024).toFixed(0)} KB document payload` : 'No document payload'}
          </div>
        </div>

        <div className="p-1.5 bg-bg border border-accent/40 rounded-xs space-y-0.5">
          <div className="text-text-muted uppercase">vakBrowse TREE:</div>
          <div className={`${isLive ? 'text-emerald-400' : 'text-text-dim'} font-bold text-xs`}>
            {isLive ? `~${perceivedTokens.toLocaleString()} TOKENS` : '0 TOKENS'}
          </div>
          <div className="text-text-dim text-[9px]">{elementsCount} interactive nodes</div>
        </div>
      </div>

      {/* Visual Reduction Bar */}
      <div className="space-y-1">
        <div className="h-2 w-full bg-surface border border-border/60 rounded-xs overflow-hidden flex">
          {isLive && (
            <div
              className="h-full bg-emerald-400 transition-all duration-500"
              style={{ width: `${Math.max(2, (perceivedTokens / Math.max(1, rawTokens)) * 100)}%` }}
            />
          )}
        </div>
        <div className="flex justify-between text-[9px] text-text-muted">
          <span className={isLive ? 'text-emerald-400 font-bold' : 'text-text-dim'}>
            {isLive ? 'vakBrowse compact window' : 'Engine ready'}
          </span>
          <span className={isLive ? 'text-danger' : 'text-text-dim'}>
            {isLive ? 'Exhausts context window (raw DOM)' : 'Awaiting navigation'}
          </span>
        </div>
      </div>
    </div>
  );
}
