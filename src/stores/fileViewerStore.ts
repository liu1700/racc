import { create } from "zustand";
import { invoke } from "@tauri-apps/api/core";
import type { FileContent, FileMatch } from "../types/file";

interface FileViewerState {
  // Overlay state
  isOpen: boolean;
  filePath: string | null;
  content: FileContent | null;
  loading: boolean;
  error: string | null;
  scrollToLine: number | null;
  highlightRange: [number, number] | null;

  // Command palette state
  isPaletteOpen: boolean;
  searchQuery: string;
  searchResults: FileMatch[];
  searchLoading: boolean;

  // Actions
  openFile: (params: {
    sessionId?: number | null;
    repoId?: number | null;
    filePath: string;
    scrollToLine?: number;
    highlightRange?: [number, number];
  }) => Promise<void>;
  closeViewer: () => void;
  openPalette: () => void;
  closePalette: () => void;
  searchFiles: (params: {
    sessionId?: number | null;
    repoId?: number | null;
    query: string;
  }) => Promise<void>;
}

export const useFileViewerStore = create<FileViewerState>((set, get) => ({
  isOpen: false,
  filePath: null,
  content: null,
  loading: false,
  error: null,
  scrollToLine: null,
  highlightRange: null,

  isPaletteOpen: false,
  searchQuery: "",
  searchResults: [],
  searchLoading: false,

  openFile: async ({ sessionId, repoId, filePath, scrollToLine, highlightRange }) => {
    set({
      isOpen: true,
      loading: true,
      error: null,
      filePath,
      scrollToLine: scrollToLine ?? null,
      highlightRange: highlightRange ?? null,
      isPaletteOpen: false,
    });

    try {
      const content = await invoke<FileContent>("read_file", {
        sessionId: sessionId ?? null,
        repoId: repoId ?? null,
        filePath,
      });
      set({ content, loading: false });
    } catch (e) {
      set({ error: String(e), loading: false });
    }
  },

  closeViewer: () => {
    set({
      isOpen: false,
      filePath: null,
      content: null,
      error: null,
      scrollToLine: null,
      highlightRange: null,
    });
  },

  openPalette: () => {
    set({ isPaletteOpen: true, searchQuery: "", searchResults: [], searchLoading: false });
  },

  closePalette: () => {
    set({ isPaletteOpen: false, searchQuery: "", searchResults: [] });
  },

  searchFiles: async ({ sessionId, repoId, query }) => {
    set({ searchQuery: query, searchLoading: true });
    try {
      const results = await invoke<FileMatch[]>("search_files", {
        sessionId: sessionId ?? null,
        repoId: repoId ?? null,
        query,
      });
      // Only update if query hasn't changed (prevent stale results)
      if (get().searchQuery === query) {
        set({ searchResults: results, searchLoading: false });
      }
    } catch {
      set({ searchResults: [], searchLoading: false });
    }
  },
}));
