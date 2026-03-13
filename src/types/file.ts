export interface FileContent {
  content: string;
  line_count: number;
  total_lines: number;
  language: string;
  encoding: string;
  file_path: string;
  is_truncated: boolean;
}

export interface FileMatch {
  relative_path: string;
  score: number;
}
