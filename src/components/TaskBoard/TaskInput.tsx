import { useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { convertFileSrc } from "@tauri-apps/api/core";
import type { DraftImage } from "../../types/task";
import { useIMEComposition } from "../../hooks/useIMEComposition";

interface Props {
  value: string;
  onChange: (value: string) => void;
  onSubmit: (description: string) => void;
  onCancel: () => void;
  repoPath: string;
  images: DraftImage[];
  onAddImage: (image: DraftImage) => void;
  onRemoveImage: (filename: string) => void;
}

export function TaskInput({
  value,
  onChange,
  onSubmit,
  onCancel,
  repoPath,
  images,
  onAddImage,
  onRemoveImage,
}: Props) {
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

  const handlePaste = async (e: React.ClipboardEvent) => {
    const items = Array.from(e.clipboardData.items);
    for (const item of items) {
      if (item.type.startsWith("image/")) {
        e.preventDefault();
        const file = item.getAsFile();
        if (!file) continue;

        const ext = item.type.split("/")[1]?.replace("jpeg", "jpg") || "png";
        const filename = `draft-${Date.now()}-${crypto.randomUUID().slice(0, 8)}.${ext}`;

        const buffer = await file.arrayBuffer();
        await invoke("save_task_image", {
          repoPath,
          filename,
          data: Array.from(new Uint8Array(buffer)),
        });

        const objectUrl = URL.createObjectURL(file);
        onAddImage({ filename, objectUrl });
      }
    }
  };

  const handleFilePick = async () => {
    const selected = await openDialog({
      multiple: true,
      filters: [
        {
          name: "Images",
          extensions: ["png", "jpg", "jpeg", "gif", "webp", "bmp"],
        },
      ],
    });
    if (!selected) return;
    const files = Array.isArray(selected) ? selected : [selected];

    for (const filePath of files) {
      const ext = filePath.split(".").pop() || "png";
      const filename = `draft-${Date.now()}-${crypto.randomUUID().slice(0, 8)}.${ext}`;

      await invoke("copy_file_to_task_images", {
        repoPath,
        sourcePath: filePath,
        filename,
      });

      const objectUrl = convertFileSrc(
        `${repoPath}/.racc/images/${filename}`
      );
      onAddImage({ filename, objectUrl });
    }
  };

  const handleRemoveImage = async (filename: string) => {
    await invoke("delete_task_image", { repoPath, filename }).catch(() => {});
    onRemoveImage(filename);
  };

  return (
    <div>
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        {...compositionProps}
        placeholder="Describe your task..."
        rows={3}
        className="w-full resize-none rounded border border-accent bg-surface-2 px-3 py-2 text-sm text-white placeholder-zinc-600 outline-none focus:border-accent-hover"
      />

      {images.length > 0 && (
        <div className="mt-1.5 flex flex-wrap gap-1.5">
          {images.map((img) => (
            <div key={img.filename} className="group relative">
              <img
                src={img.objectUrl}
                alt=""
                className="h-12 w-12 rounded border border-surface-3 object-cover"
              />
              <button
                onClick={() => handleRemoveImage(img.filename)}
                className="absolute -right-1 -top-1 hidden h-4 w-4 items-center justify-center rounded-full bg-status-error text-[10px] text-white group-hover:flex"
              >
                x
              </button>
            </div>
          ))}
        </div>
      )}

      <div className="mt-1 flex justify-end gap-1.5">
        <button
          type="button"
          onClick={handleFilePick}
          className="flex items-center gap-1 rounded bg-surface-2 px-2 py-0.5 text-xs text-zinc-400 hover:text-zinc-200"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            className="h-3.5 w-3.5"
          >
            <path
              fillRule="evenodd"
              d="M1 5.25A2.25 2.25 0 0 1 3.25 3h13.5A2.25 2.25 0 0 1 19 5.25v9.5A2.25 2.25 0 0 1 16.75 17H3.25A2.25 2.25 0 0 1 1 14.75v-9.5Zm1.5 5.81v3.69c0 .414.336.75.75.75h13.5a.75.75 0 0 0 .75-.75v-2.69l-2.22-2.219a.75.75 0 0 0-1.06 0l-1.91 1.909-4.97-4.969a.75.75 0 0 0-1.06 0L2.5 11.06ZM12.75 7a1.25 1.25 0 1 1 2.5 0 1.25 1.25 0 0 1-2.5 0Z"
              clipRule="evenodd"
            />
          </svg>
        </button>
        <button
          onClick={() => value.trim() && onSubmit(value.trim())}
          disabled={!value.trim()}
          className="flex items-center gap-1 rounded bg-accent/15 px-2 py-0.5 text-xs text-accent hover:bg-accent/25 disabled:cursor-not-allowed disabled:opacity-30"
        >
          <svg
            xmlns="http://www.w3.org/2000/svg"
            viewBox="0 0 20 20"
            fill="currentColor"
            className="h-3.5 w-3.5"
          >
            <path
              fillRule="evenodd"
              d="M16.704 4.153a.75.75 0 0 1 .143 1.052l-8 10.5a.75.75 0 0 1-1.127.075l-4.5-4.5a.75.75 0 0 1 1.06-1.06l3.894 3.893 7.48-9.817a.75.75 0 0 1 1.05-.143Z"
              clipRule="evenodd"
            />
          </svg>
          Submit
        </button>
      </div>
    </div>
  );
}
