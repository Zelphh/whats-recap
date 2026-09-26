// Espelho dos tipos serializados pelo backend Rust (serde, camelCase).

export type MessageKind =
  | "text" | "sticker" | "audio" | "image" | "video" | "doc" | "deleted" | "system" | "media_hidden";

export interface ImportSummary {
  sourceName: string;
  hasMedia: boolean;
  platform: "android" | "ios";
  dateOrder: "dmy" | "mdy" | "ymd";
  authors: string[];
  messageCount: number;
  firstTs: number;
  lastTs: number;
  sessionCount: number;
  sessionGapSecs: number;
  importedAt: number;
  orphanLines: number;
  invalidDates: number;
}

export interface ConversationInfo {
  id: string;
  summary: ImportSummary;
}

export interface MessageRow {
  id: number;
  ts: number;
  author: string | null;
  kind: MessageKind;
  text: string | null;
  mediaFile: string | null;
  sessionId: number;
}

export interface Count {
  item: string;
  count: number;
}

export interface Totals {
  messages: number;
  words: number;
  textMessages: number;
  avgWordsPerMessage: number;
  stickers: number;
  audios: number;
  images: number;
  videos: number;
  docs: number;
  mediaHidden: number;
  deleted: number;
}

export interface LongestMessage {
  id: number;
  author: number;
  ts: number;
  words: number;
  chars: number;
  preview: string;
}

export interface Series {
  start: string;
  values: number[][];
}

export interface ExclusiveWord {
  word: string;
  count: number;
  otherCount: number;
  ratio: number | null;
  zScore: number;
}

export interface ResponseStats {
  count: number;
  meanSecs: number;
  medianSecs: number;
  buckets: [number, number, number, number];
}

export interface Stats {
  version: number;
  authors: string[];
  hasMedia: boolean;
  totals: Totals;
  perAuthor: Totals[];
  longestMessage: LongestMessage | null;
  days: {
    firstDate: string | null;
    lastDate: string | null;
    activeDays: number;
    spanDays: number;
    activePct: number;
  };
  daily: Series;
  monthly: Series;
  weekday: {
    totals: number[][];
    occurrences: number[];
    avgPerOccurrence: number[];
    mostActive: number;
    leastActive: number;
  };
  weekdayChartText: string;
  hourly: number[][];
  topWords: Count[];
  topWordsByAuthor: Count[][];
  topEmojis: Count[];
  topEmojisByAuthor: Count[][];
  exclusiveWords: { onlyYou: ExclusiveWord[]; muchMore: ExclusiveWord[] }[];
  responseTimes: ResponseStats[];
  fastestResponder: number | null;
  /** Figurinhas e áudios; `null` no modo sem mídia. */
  media: MediaStats | null;
}

export interface StickerCount {
  file: string;
  count: number;
  variants: number;
}

export interface AudioStats {
  count: number;
  withDuration: number;
  totalMs: number;
  avgMs: number;
}

export interface MediaStats {
  topStickers: StickerCount[];
  topStickersByAuthor: StickerCount[][];
  distinctStickers: number;
  missingStickers: number;
  audio: AudioStats[];
}

export interface StatsSettings {
  sessionGapSecs: number;
  minExclusiveFreq: number;
  stopwords: string[];
  stripAccents: boolean;
  groupSkinTones: boolean;
  stickerMaxDistance: number;
}

export interface ImportProgressEvent {
  phase: "detect" | "parse" | "index" | "media" | "stats";
  fraction: number;
  messages: number;
}
