import React, { useState } from 'react';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  url: string;
  selectedRef: string | null;
}

export function ExportCodeModal({ isOpen, onClose, url, selectedRef }: Props) {
  const [activeLang, setActiveLang] = useState<'python' | 'cli' | 'mcp' | 'curl'>('python');
  const [copied, setCopied] = useState(false);

  if (!isOpen) return null;

  const targetUrl = url || 'https://en.wikipedia.org/wiki/Artificial_intelligence';
  const targetRef = selectedRef || '@e1';

  const snippets = {
    python: `# Install: pip install vakbrowse
from vakbrowse import Session

s = Session()
# 1. Open target website under stealth profile
sid, initial_url = s.open("${targetUrl}", stealth=True)

# 2. Click target element via trusted mouse event
result = s.click(sid, "${targetRef}")

# 3. Extract readable main-content text (<500 tokens)
content = s.extract(sid)
print(f"Title: {content['title']}")
print(content["text"][:300])

# 4. Clean up
s.close(sid)
`,

    cli: `# vak CLI Commands
# 1. Launch session with stealth profile
vak open ${targetUrl} --stealth

# 2. Inspect compact perception tree
vak snapshot

# 3. Click target element
vak click ${targetRef}

# 4. Extract token-cheap readable markdown
vak extract
`,

    mcp: `// MCP (Model Context Protocol) Client Tool Call
// Used in Claude Code / Cursor / opencode
{
  "name": "browser_open",
  "arguments": {
    "url": "${targetUrl}",
    "stealth": true
  }
}

// Follow-up click action:
{
  "name": "browser_click",
  "arguments": {
    "ref": "${targetRef}"
  }
}
`,

    curl: `# Direct REST API Call to vakd-rest daemon
# 1. Open session
curl -X POST http://localhost:7788/playground/rpc \\
  -H "Content-Type: application/json" \\
  -d '{"type":"open","options":{"url":"${targetUrl}","stealth":true,"headless":true}}'

# 2. Click element
curl -X POST http://localhost:7788/playground/rpc \\
  -H "Content-Type: application/json" \\
  -d '{"type":"act","session":"s1","action":{"type":"click","ref":"${targetRef}"}}'

# 3. Extract readable text
curl -X POST http://localhost:7788/playground/rpc \\
  -H "Content-Type: application/json" \\
  -d '{"type":"act","session":"s1","action":{"type":"extract"}}'
`,
  };

  const handleCopy = () => {
    navigator.clipboard.writeText(snippets[activeLang]);
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  return (
    <div className="fixed inset-0 z-50 bg-black/70 backdrop-blur-xs flex items-center justify-center p-4 font-mono text-xs">
      <div className="bg-surface border border-border-strong rounded-xs shadow-2xl w-full max-w-2xl overflow-hidden flex flex-col max-h-[85vh]">
        {/* Header */}
        <div className="p-3 bg-card border-b border-border flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="w-2 h-2 rounded-full bg-accent" />
            <span className="font-bold text-bone tracking-wider uppercase">EXPORT RUNNABLE CODE</span>
          </div>
          <button
            onClick={onClose}
            className="text-text-muted hover:text-white px-2 py-0.5 rounded-xs"
          >
            ✕
          </button>
        </div>

        {/* Tab Selection */}
        <div className="flex border-b border-border bg-bg text-[11px]">
          <button
            onClick={() => setActiveLang('python')}
            className={`px-4 py-2 font-bold transition-colors ${
              activeLang === 'python' ? 'bg-surface text-accent border-b-2 border-accent' : 'text-text-dim hover:text-text'
            }`}
          >
            PYTHON SDK
          </button>
          <button
            onClick={() => setActiveLang('cli')}
            className={`px-4 py-2 font-bold transition-colors ${
              activeLang === 'cli' ? 'bg-surface text-accent border-b-2 border-accent' : 'text-text-dim hover:text-text'
            }`}
          >
            CLI (vak)
          </button>
          <button
            onClick={() => setActiveLang('mcp')}
            className={`px-4 py-2 font-bold transition-colors ${
              activeLang === 'mcp' ? 'bg-surface text-accent border-b-2 border-accent' : 'text-text-dim hover:text-text'
            }`}
          >
            MCP TOOLS
          </button>
          <button
            onClick={() => setActiveLang('curl')}
            className={`px-4 py-2 font-bold transition-colors ${
              activeLang === 'curl' ? 'bg-surface text-accent border-b-2 border-accent' : 'text-text-dim hover:text-text'
            }`}
          >
            cURL / REST
          </button>
        </div>

        {/* Snippet Display */}
        <div className="p-4 flex-1 min-h-0 overflow-y-auto bg-bg">
          <pre className="p-3 bg-[#0d0d10] border border-border rounded-xs text-[11px] text-bone font-mono overflow-x-auto leading-relaxed whitespace-pre shadow-te-inset">
            {snippets[activeLang]}
          </pre>
        </div>

        {/* Footer */}
        <div className="p-3 bg-card border-t border-border flex items-center justify-between">
          <span className="text-[10px] text-text-muted">
            Code matches active session settings: URL = <span className="text-bone">{targetUrl}</span>
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={handleCopy}
              className="btn-primary py-1 px-4 text-xs font-bold uppercase tracking-wider"
            >
              {copied ? '✓ COPIED TO CLIPBOARD' : 'COPY SNIPPET 📋'}
            </button>
            <button
              onClick={onClose}
              className="btn-ghost py-1 px-3 text-xs"
            >
              CLOSE
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
