import React from 'react';

export type PageTab = 'overview' | 'use-cases' | 'architecture' | 'economics' | 'docs' | 'pricing' | 'playground';

interface HeaderProps {
  currentTab: PageTab;
  onSelectTab: (tab: PageTab) => void;
  activeSessionCount: number;
}

export function Header({ currentTab, onSelectTab, activeSessionCount }: HeaderProps) {
  const navItems: { id: PageTab; label: string; code: string }[] = [
    { id: 'overview', label: 'OVERVIEW', code: '01' },
    { id: 'use-cases', label: 'USE CASES', code: '02' },
    { id: 'architecture', label: 'ARCHITECTURE', code: '03' },
    { id: 'economics', label: 'ECONOMICS', code: '04' },
    { id: 'docs', label: 'DOCS & SDK', code: '05' },
    { id: 'pricing', label: 'COMMERCIAL', code: '06' },
  ];

  return (
    <header className="border-b border-border bg-bg/95 backdrop-blur sticky top-0 z-50 select-none">
      {/* Top telemetry bar */}
      <div className="border-b border-border/60 px-4 py-1 flex items-center justify-between text-[11px] font-mono text-text-dim">
        <div className="flex items-center gap-4">
          <span className="flex items-center gap-1.5 text-text">
            <span className="inline-block w-2 h-2 rounded-full bg-success animate-pulse shadow-[0_0_8px_rgba(16,185,129,0.7)]" />
            <span className="tracking-wider">VAKBROWSE ENGINE // v0.4.0</span>
          </span>
          <span className="hidden md:inline text-text-muted">|</span>
          <span className="hidden md:inline">
            TOKENS: <span className="text-yellow font-semibold">&lt;500 / PG</span>
          </span>
          <span className="hidden lg:inline text-text-muted">|</span>
          <span className="hidden lg:inline">
            LATENCY: <span className="text-text font-semibold">42ms MEDIAN</span>
          </span>
          <span className="hidden xl:inline text-text-muted">|</span>
          <span className="hidden xl:inline">
            BACKENDS: <span className="text-bone">CDP + DOM-QUICKJS</span>
          </span>
        </div>
        
        <div className="flex items-center gap-3">
          <span className="badge-dim">
            SESSIONS: <span className="text-accent ml-1 font-semibold">{activeSessionCount} ACTIVE</span>
          </span>
          <a
            href="https://github.com/nranjan2code/vakBrowse"
            target="_blank"
            rel="noreferrer"
            className="hover:text-accent transition-colors flex items-center gap-1"
          >
            <span>GITHUB ↗</span>
          </a>
        </div>
      </div>

      {/* Main navigation bar */}
      <div className="px-4 py-2.5 flex items-center justify-between">
        {/* Logo / Brand */}
        <div 
          onClick={() => onSelectTab('overview')}
          className="cursor-pointer flex items-center gap-2.5 group"
        >
          <div className="w-7 h-7 rounded-xs bg-card border border-border-strong flex items-center justify-center font-mono font-bold text-xs text-accent group-hover:border-accent transition-colors shadow-te-inset">
            vB
          </div>
          <div>
            <div className="flex items-center gap-2">
              <span className="font-mono font-bold text-sm tracking-wider text-bone group-hover:text-accent transition-colors">
                vakBrowse
              </span>
              <span className="text-[10px] font-mono px-1 bg-surface border border-border text-text-dim rounded-xs">
                CORE
              </span>
            </div>
            <div className="text-[10px] font-mono text-text-dim leading-none">
              THE AGENT-NATIVE BROWSER
            </div>
          </div>
        </div>

        {/* Tab switchers */}
        <nav className="hidden md:flex items-center gap-1 bg-card/70 border border-border p-1 rounded-sm">
          {navItems.map((item) => {
            const isActive = currentTab === item.id;
            return (
              <button
                key={item.id}
                onClick={() => onSelectTab(item.id)}
                className={`px-3 py-1 text-xs font-mono tracking-wider transition-all flex items-center gap-1.5 rounded-xs ${
                  isActive
                    ? 'bg-surface-elevated text-bone border border-border-strong font-semibold shadow-xs'
                    : 'text-text-dim hover:text-text hover:bg-surface/50'
                }`}
              >
                <span className={`text-[10px] ${isActive ? 'text-accent' : 'text-text-muted'}`}>
                  {item.code}
                </span>
                <span>{item.label}</span>
              </button>
            );
          })}
        </nav>

        {/* Action Button: Launch Playground */}
        <div className="flex items-center gap-2">
          <button
            onClick={() => onSelectTab(currentTab === 'playground' ? 'overview' : 'playground')}
            className={`px-4 py-1.5 rounded-xs text-xs font-mono font-bold tracking-wider uppercase transition-all flex items-center gap-2 ${
              currentTab === 'playground'
                ? 'bg-surface border border-accent text-accent'
                : 'bg-accent hover:bg-accent-hover text-white shadow-te-button active:translate-y-0.5'
            }`}
          >
            <span className={`w-2 h-2 rounded-full ${currentTab === 'playground' ? 'bg-accent animate-ping' : 'bg-white'}`} />
            <span>{currentTab === 'playground' ? 'EXIT PLAYGROUND' : 'PLAYGROUND'}</span>
            <span className="text-[10px] opacity-70">⚡</span>
          </button>
        </div>
      </div>

      {/* Mobile nav drawer row */}
      <div className="md:hidden border-t border-border px-3 py-2 flex items-center gap-1 overflow-x-auto text-[11px] font-mono">
        {navItems.map((item) => (
          <button
            key={item.id}
            onClick={() => onSelectTab(item.id)}
            className={`px-2.5 py-1 whitespace-nowrap rounded-xs ${
              currentTab === item.id ? 'bg-accent text-white font-bold' : 'text-text-dim hover:text-text'
            }`}
          >
            {item.label}
          </button>
        ))}
      </div>
    </header>
  );
}
