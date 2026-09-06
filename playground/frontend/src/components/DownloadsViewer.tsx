import React, { useState, useEffect } from 'react';

interface Props {
  onGetDownloads: () => Promise<any[]>;
  onSetDownloadDir: (dir: string) => Promise<any>;
  loading: boolean;
  disabled: boolean;
}

interface DownloadItem {
  path: string;
  bytes: number;
}

export function DownloadsViewer({ onGetDownloads, onSetDownloadDir, loading, disabled }: Props) {
  const [downloads, setDownloads] = useState<DownloadItem[]>([]);
  const [fetching, setFetching] = useState(false);
  const [statusMsg, setStatusMsg] = useState<string | null>(null);
  const [customDir, setCustomDir] = useState('');
  const [showDirForm, setShowDirForm] = useState(false);

  const loadDownloads = async () => {
    if (disabled) return;
    setFetching(true);
    setStatusMsg(null);
    try {
      const res = await onGetDownloads();
      const list = Array.isArray(res) ? res : [];
      setDownloads(list);
      setStatusMsg(`Loaded ${list.length} download record(s)`);
    } catch (err: any) {
      setStatusMsg(`Failed to query downloads: ${err.message}`);
    } finally {
      setFetching(false);
    }
  };

  useEffect(() => {
    loadDownloads();
  }, [disabled]);

  const handleSetDir = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!customDir.trim() || disabled) return;
    setFetching(true);
    try {
      await onSetDownloadDir(customDir.trim());
      setStatusMsg(`Download directory set to: ${customDir.trim()}`);
      setShowDirForm(false);
      await loadDownloads();
    } catch (err: any) {
      setStatusMsg(`Failed to set download dir: ${err.message}`);
    } finally {
      setFetching(false);
    }
  };

  const formatBytes = (bytes: number) => {
    if (bytes === 0) return '0 B';
    const k = 1024;
    const sizes = ['B', 'KB', 'MB', 'GB'];
    const i = Math.floor(Math.log(bytes) / Math.log(k));
    return `${parseFloat((bytes / Math.pow(k, i)).toFixed(1))} ${sizes[i]}`;
  };

  return (
    <div className="flex flex-col h-full space-y-3 font-mono text-xs">
      {/* Header */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <span className="font-bold text-bone">DOWNLOAD MANAGER</span>
          <span className="badge-dim">{downloads.length} FILES</span>
        </div>
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setShowDirForm(!showDirForm)}
            disabled={disabled || loading || fetching}
            className="btn-secondary py-0.5 px-2 text-[10px]"
            title="Configure target download directory"
          >
            {showDirForm ? 'CANCEL' : '📁 SET DIR'}
          </button>
          <button
            onClick={loadDownloads}
            disabled={disabled || loading || fetching}
            className="btn-primary py-0.5 px-2.5 text-[10px]"
          >
            {fetching ? 'SYNCING...' : '↻ REFRESH'}
          </button>
        </div>
      </div>

      {statusMsg && (
        <div className="text-[10px] text-accent border border-border bg-surface px-2 py-1 rounded-xs">
          {statusMsg}
        </div>
      )}

      {/* Set Directory Form */}
      {showDirForm && (
        <form onSubmit={handleSetDir} className="p-3 bg-surface border border-border rounded-xs space-y-2">
          <div className="font-bold text-bone text-[11px] uppercase">CONFIGURE DOWNLOAD DIRECTORY</div>
          <p className="text-[10px] text-text-dim">
            Direct file downloads triggered by link clicks or automation into a custom workspace directory.
          </p>
          <div className="flex items-center gap-1.5">
            <input
              type="text"
              required
              value={customDir}
              onChange={(e) => setCustomDir(e.target.value)}
              placeholder="/tmp/vakbrowse/downloads"
              className="input-sm flex-1 text-[11px]"
            />
            <button
              type="submit"
              disabled={loading || fetching || !customDir.trim()}
              className="btn-primary py-0.5 px-3 text-[11px] shrink-0"
            >
              SAVE
            </button>
          </div>
        </form>
      )}

      {/* Downloads List Table */}
      <div className="flex-1 min-h-[220px] bg-bg border border-border rounded-xs overflow-y-auto shadow-te-inset">
        {downloads.length === 0 ? (
          <div className="p-6 text-center text-text-dim text-[11px] space-y-1">
            <div className="text-bone font-bold uppercase">NO DOWNLOADS RECORDED</div>
            <p>
              Files downloaded during this session will be indexed here with byte counts and local disk paths via <code className="text-accent">Action::Downloads</code>.
            </p>
          </div>
        ) : (
          <div className="divide-y divide-border/60">
            {downloads.map((item, idx) => {
              const filename = item.path.split('/').pop() || item.path;
              return (
                <div key={`${item.path}-${idx}`} className="p-2.5 hover:bg-surface/40 transition-colors space-y-1">
                  <div className="flex items-center justify-between">
                    <span className="font-bold text-bone truncate max-w-[200px]" title={filename}>
                      {filename}
                    </span>
                    <span className="badge-green text-[9px] font-bold">
                      {formatBytes(item.bytes)}
                    </span>
                  </div>
                  <div className="text-[10px] text-text-dim truncate font-mono bg-card px-1.5 py-0.5 rounded-xs" title={item.path}>
                    {item.path}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
