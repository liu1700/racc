export function CostTracker() {
  return (
    <div className="border-b border-surface-3 bg-surface-1 px-4 py-3">
      <h2 className="text-xs font-semibold uppercase tracking-wider text-zinc-400">
        Cost
      </h2>
      <div className="mt-2 grid grid-cols-2 gap-3">
        <div>
          <p className="text-xs text-zinc-500">Session</p>
          <p className="text-lg font-semibold text-white">$0.00</p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Total</p>
          <p className="text-lg font-semibold text-white">$0.00</p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Input tokens</p>
          <p className="text-sm text-zinc-300">0</p>
        </div>
        <div>
          <p className="text-xs text-zinc-500">Output tokens</p>
          <p className="text-sm text-zinc-300">0</p>
        </div>
      </div>
    </div>
  );
}
