import { useRef, useEffect } from "react";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (description: string) => void;
  onCancel: () => void;
}

export function TaskInput({ value, onChange, onSubmit, onCancel }: Props) {
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey && value.trim()) {
      e.preventDefault();
      onSubmit(value.trim());
    }
    if (e.key === "Escape") {
      onCancel();
    }
  };

  return (
    <textarea
      ref={textareaRef}
      value={value}
      onChange={(e) => onChange(e.target.value)}
      onKeyDown={handleKeyDown}
      placeholder="Describe your task..."
      rows={3}
      className="w-full resize-none rounded border border-accent bg-surface-2 px-3 py-2 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent-hover"
    />
  );
}
