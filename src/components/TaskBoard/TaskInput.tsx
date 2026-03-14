import { useState, useRef, useEffect } from "react";

interface Props {
  onSubmit: (description: string) => void;
  onCancel: () => void;
}

export function TaskInput({ onSubmit, onCancel }: Props) {
  const [value, setValue] = useState("");
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && value.trim()) {
      onSubmit(value.trim());
      setValue("");
    }
    if (e.key === "Escape") {
      onCancel();
    }
  };

  return (
    <input
      ref={inputRef}
      type="text"
      value={value}
      onChange={(e) => setValue(e.target.value)}
      onKeyDown={handleKeyDown}
      onBlur={onCancel}
      placeholder="Describe your task..."
      className="w-full rounded border border-accent bg-surface-2 px-3 py-2 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent-hover"
    />
  );
}
