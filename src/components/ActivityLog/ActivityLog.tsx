export function ActivityLog() {
  return (
    <div className="flex flex-1 flex-col overflow-hidden border-t border-surface-3">
      <div className="border-b border-surface-3 px-4 py-2">
        <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
          Activity Log
        </h2>
      </div>
      <div className="flex-1 overflow-y-auto p-3">
        <div className="flex items-center justify-center py-8 text-xs text-zinc-600">
          Agent activity will appear here.
        </div>
      </div>
    </div>
  );
}
