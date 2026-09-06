import React from 'react';
import type { PageTab } from '../components/Header';

interface PricingPageProps {
  onNavigate: (tab: PageTab) => void;
}

export function PricingPage({ onNavigate }: PricingPageProps) {
  return (
    <div className="max-w-7xl mx-auto px-4 sm:px-6 py-10 space-y-14">
      {/* Header */}
      <div className="space-y-3 text-center max-w-2xl mx-auto">
        <div className="badge-orange mx-auto">COMMERCIAL ARCHITECTURE // FLEET LICENSING</div>
        <h1 className="text-3xl sm:text-4xl font-bold font-sans text-bone">
          Predictable Pricing for Autonomous Agent Fleets
        </h1>
        <p className="text-sm sm:text-base text-text-dim font-sans">
          Whether you are running a single desktop research agent or deploying thousands of autonomous scrapers across an enterprise grid.
        </p>
      </div>

      {/* Tier Cards Grid */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        {/* Tier 1: Core / Open Source */}
        <div className="te-panel rounded-xs border-border p-6 flex flex-col justify-between space-y-6">
          <div className="space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-border">
              <span className="font-mono text-xs font-bold text-bone uppercase tracking-wider">
                01 // COMMUNITY CORE
              </span>
              <span className="badge-dim">OPEN SOURCE</span>
            </div>

            <div className="font-mono">
              <div className="text-3xl font-bold text-bone">$0</div>
              <div className="text-[11px] text-text-muted mt-0.5">PERPETUAL MIT / APACHE-2.0</div>
            </div>

            <p className="text-xs text-text-dim font-sans leading-relaxed">
              Complete local engine runtime. Self-hosted on macOS, Linux, or inside your own Docker infrastructure with zero telemetry.
            </p>

            <ul className="space-y-2 text-xs font-mono text-text-dim pt-4 border-t border-border">
              <li className="flex items-center gap-2">
                <span className="text-emerald-400">✓</span>
                <span>Full CDP + DOM-QuickJS Backends</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-emerald-400">✓</span>
                <span>Compact &lt;500 Token Perception Engine</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-emerald-400">✓</span>
                <span>34 WebMCP Tools (Claude & Cursor)</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-emerald-400">✓</span>
                <span>Local UDS & HTTP REST Daemons</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-emerald-400">✓</span>
                <span>Python SDK & C-ABI Library</span>
              </li>
            </ul>
          </div>

          <a
            href="https://github.com/nranjan2code/vakBrowse"
            target="_blank"
            rel="noreferrer"
            className="btn-secondary w-full text-center"
          >
            VIEW GITHUB REPO ↗
          </a>
        </div>

        {/* Tier 2: Pro Agent Node */}
        <div className="te-panel rounded-xs border-2 border-accent p-6 flex flex-col justify-between space-y-6 shadow-te-button relative">
          <div className="absolute top-0 right-0 bg-accent text-white font-mono text-[9px] px-2 py-0.5 uppercase tracking-widest font-bold">
            RECOMMENDED FOR TEAMS
          </div>

          <div className="space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-border">
              <span className="font-mono text-xs font-bold text-accent uppercase tracking-wider">
                02 // PRO AGENT NODE
              </span>
              <span className="badge-orange">DEVELOPER SUITE</span>
            </div>

            <div className="font-mono">
              <div className="text-3xl font-bold text-bone">
                $49<span className="text-xs text-text-muted font-normal ml-1">/mo per node</span>
              </div>
              <div className="text-[11px] text-accent mt-0.5">MANAGED RUNTIME & STEALTH MESH</div>
            </div>

            <p className="text-xs text-text-dim font-sans leading-relaxed">
              Hardened remote browser runtime with managed residential proxy endpoints, automated CAPTCHA recovery, and warm session pooling.
            </p>

            <ul className="space-y-2 text-xs font-mono text-text pt-4 border-t border-border">
              <li className="flex items-center gap-2">
                <span className="text-accent">✓</span>
                <span>All Open Core Capabilities</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-accent">✓</span>
                <span>Managed Rotating Residential Proxy Pool</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-accent">✓</span>
                <span>Hardware Stealth Profile Generator</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-accent">✓</span>
                <span>Warm Standby Browser Session Pool</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-accent">✓</span>
                <span>Automated Bot-Wall Recovery Fallbacks</span>
              </li>
            </ul>
          </div>

          <button
            onClick={() => onNavigate('playground')}
            className="btn-primary w-full text-center"
          >
            START 14-DAY TRIAL ›
          </button>
        </div>

        {/* Tier 3: Enterprise Fleet */}
        <div className="te-panel rounded-xs border-border p-6 flex flex-col justify-between space-y-6">
          <div className="space-y-4">
            <div className="flex items-center justify-between pb-3 border-b border-border">
              <span className="font-mono text-xs font-bold text-bone uppercase tracking-wider">
                03 // ENTERPRISE FLEET
              </span>
              <span className="badge-yellow">CUSTOM GRID</span>
            </div>

            <div className="font-mono">
              <div className="text-3xl font-bold text-bone">CUSTOM</div>
              <div className="text-[11px] text-text-muted mt-0.5">DEDICATED ISOLATED INFRASTRUCTURE</div>
            </div>

            <p className="text-xs text-text-dim font-sans leading-relaxed">
              Distributed browser grids for organizations processing millions of daily pages. Dedicated VPC deployments with compliance logging.
            </p>

            <ul className="space-y-2 text-xs font-mono text-text-dim pt-4 border-t border-border">
              <li className="flex items-center gap-2">
                <span className="text-yellow">✓</span>
                <span>Private VPC & Kubernetes Cluster Helm Charts</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-yellow">✓</span>
                <span>Unlimited Concurrency & Dynamic Autoscaling</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-yellow">✓</span>
                <span>SOC2 Type II & HIPAA Compliance Log Audit</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-yellow">✓</span>
                <span>Custom Stealth Fingerprint Synthesis</span>
              </li>
              <li className="flex items-center gap-2">
                <span className="text-yellow">✓</span>
                <span>24/7 Dedicated Support & 99.99% SLA</span>
              </li>
            </ul>
          </div>

          <button
            onClick={() => onNavigate('docs')}
            className="btn-secondary w-full text-center"
          >
            CONTACT ARCHITECTURE TEAM ›
          </button>
        </div>
      </div>

      {/* Feature Matrix Table */}
      <div className="te-panel rounded-xs border-border p-6 space-y-4">
        <h3 className="text-base font-bold font-sans text-bone">
          Technical Feature Comparison Matrix
        </h3>

        <div className="overflow-x-auto">
          <table className="w-full text-left font-mono text-xs">
            <thead>
              <tr className="border-b border-border text-text-muted uppercase text-[10px]">
                <th className="py-2.5 px-3">CAPABILITY</th>
                <th className="py-2.5 px-3">COMMUNITY CORE</th>
                <th className="py-2.5 px-3">PRO AGENT NODE</th>
                <th className="py-2.5 px-3">ENTERPRISE FLEET</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-border/60 text-text-dim">
              <tr>
                <td className="py-2 px-3 text-bone">Perception Snapshot Compression</td>
                <td className="py-2 px-3 text-emerald-400">&lt;500 tokens</td>
                <td className="py-2 px-3 text-emerald-400">&lt;500 tokens</td>
                <td className="py-2 px-3 text-emerald-400">&lt;500 tokens</td>
              </tr>
              <tr>
                <td className="py-2 px-3 text-bone">Dual Engine Seam (CDP + DOM/JS)</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
              </tr>
              <tr>
                <td className="py-2 px-3 text-bone">WebMCP 34 Tools (Stdio Normalizer)</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
                <td className="py-2 px-3 text-emerald-400">Included</td>
              </tr>
              <tr>
                <td className="py-2 px-3 text-bone">Rotating Proxy Pool</td>
                <td className="py-2 px-3 text-text-muted">Self-provided</td>
                <td className="py-2 px-3 text-accent font-semibold">Managed residential</td>
                <td className="py-2 px-3 text-accent font-semibold">Dedicated worldwide</td>
              </tr>
              <tr>
                <td className="py-2 px-3 text-bone">Stealth Fingerprint Synthesis</td>
                <td className="py-2 px-3 text-text-dim">Basic evasions</td>
                <td className="py-2 px-3 text-accent font-semibold">Advanced biometric</td>
                <td className="py-2 px-3 text-accent font-semibold">Custom synthesized</td>
              </tr>
              <tr>
                <td className="py-2 px-3 text-bone">VPC Deployment & Audit Logs</td>
                <td className="py-2 px-3 text-text-muted">—</td>
                <td className="py-2 px-3 text-text-muted">—</td>
                <td className="py-2 px-3 text-emerald-400 font-semibold">SOC2 Type II</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </div>
  );
}
