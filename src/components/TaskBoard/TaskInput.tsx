import { useRef, useEffect } from "react";
import { useIMEComposition } from "../../hooks/useIMEComposition";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (description: string) => void;
  onCancel: () => void;
}

export function TaskInput({ value, onChange, onSubmit, onCancel }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const { isComposingRef, compositionProps } = useIMEComposition();

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey && !isComposingRef.current && value.trim()) {
      e.preventDefault();
      onSubmit(value.trim());
    }
    if (e.key === "Escape") {
      onCancel();
    }
  };

  return (
    <div>
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        {...compositionProps}
        placeholder="Describe your task..."
        rows={3}
        className="w-full resize-none rounded border border-accent bg-surface-2 px-3 py-2 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent-hover"
      />
      <div className="mt-1 flex justify-end">
        <button
          onClick={() => value.trim() && onSubmit(value.trim())}
          disabled={!value.trim()}
          className="flex items-center gap-1 rounded bg-accent/15 px-2 py-0.5 text-xs text-accent hover:bg-accent/25 disabled:opacity-30 disabled:cursor-not-allowed"
        >
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" className="h-3.5 w-3.5">
            <path fillRule="evenodd" d="M16.704 4.153a.75.75 0 0 1 .143 1.052l-8 10.5a.75.75 0 0 1-1.127.075l-4.5-4.5a.75.75 0 0 1 1.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 0 1 1.05-.143Z" clipRule="evenodd" />
          </svg>
          Submit
        </button>
      </div>
    </div>
  );
}
