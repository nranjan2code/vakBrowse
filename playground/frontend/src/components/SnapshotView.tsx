import type { Snapshot, SnapshotNode } from '../lib/types';

interface Props {
  snapshot: Snapshot | null;
  onElementClick: (node: SnapshotNode) => void;
  selectedRef: string | null;
}

export function SnapshotView({ snapshot, onElementClick, selectedRef }: Props) {
  if (!snapshot) {
    return (
      <div className="flex-1 flex items-center justify-center text-text-dim">
        <p>No snapshot loaded. Open a session and navigate to get started.</p>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="mb-4 border-b border-border pb-2">
        <h2 className="text-lg font-medium text-text">
          {snapshot.title || '(no title)'}
        </h2>
        <p className="text-sm text-text-dim break-all">{snapshot.url}</p>
      </div>
      {snapshot.elements.length === 0 ? (
        <p className="text-text-dim">No interactive elements found on this page.</p>
      ) : (
        <div className="grid gap-2">
          {snapshot.elements.map((el) => (
            <ElementRow
              key={el.ref}
              node={el}
              selected={el.ref === selectedRef}
              onClick={() => onElementClick(el)}
            />
          ))}
        </div>
      )}
    </div>
  );
}

interface ElementRowProps {
  node: SnapshotNode;
  selected: boolean;
  onClick: () => void;
}

const roleIcon: Record<string, string> = {
  link: '\u{1F516}', button: '\u25B8', textbox: '\u{1F4DD}',
  checkbox: '\u2610', radio: '\u25CB', combobox: '\u25BC',
  menuitem: '\u2022', heading: '#', table: '\u25A1',
  cell: '[]', list: '\u2630', listitem: '\u2022',
};

function ElementRow({ node, selected, onClick }: ElementRowProps) {
  const icon = roleIcon[node.role] || '?';
  return (
    <div
      onClick={onClick}
      className={`
        flex items-center gap-3 p-2 rounded cursor-pointer transition-colors
        ${selected
          ? 'bg-accent/20 border border-accent'
          : 'bg-card border border-border hover:bg-card/70'}
      `}
    >
      <span className="text-sm font-mono text-accent w-12 text-center">{node.ref}</span>
      <span className="text-xs w-6 text-center">{icon}</span>
      <div className="flex-1 min-w-0">
        <span className="text-sm text-text">{node.name || '(no name)'}</span>
        {node.value && (
          <span className="text-xs text-text-dim ml-2 truncate">
            value: "{node.value}"
          </span>
        )}
      </div>
      <span className="text-xs text-text-dim uppercase">{node.role}</span>
    </div>
  );
}
