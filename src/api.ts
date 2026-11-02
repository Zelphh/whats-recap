import { invoke } from "@tauri-apps/api/core";
import type { ChatAnswer, ConversationInfo, ImportSummary, ListFilter, LlmStatus, MessageRow, Stats, StatsSettings } from "./types";

export const api = {
  listConversations: () => invoke<ConversationInfo[]>("list_conversations"),
  importConversation: (path: string) => invoke<ConversationInfo>("import_conversation", { path }),
  cancelImport: () => invoke<void>("cancel_import"),
  stickerThumb: (id: string, file: string) => invoke<ArrayBuffer>("get_sticker_thumb", { id, file }),
  deleteConversation: (id: string) => invoke<void>("delete_conversation", { id }),
  getSummary: (id: string) => invoke<ImportSummary>("get_summary", { id }),
  getStats: (id: string) => invoke<Stats>("get_stats", { id }),
  getSettings: (id: string) => invoke<StatsSettings>("get_settings", { id }),
  defaultSettings: () => invoke<StatsSettings>("default_settings"),
  updateSettings: (id: string, settings: StatsSettings) => invoke<Stats>("update_settings", { id, settings }),
  getMessages: (id: string, startId: number, limit: number) =>
    invoke<MessageRow[]>("get_messages", { id, startId, limit }),
  searchMessages: (id: string, query: string, limit = 200) =>
    invoke<MessageRow[]>("search_messages", { id, query, limit }),
  listMessages: (id: string, filter: ListFilter, limit = 1000) =>
    invoke<MessageRow[]>("list_messages", { id, filter, limit }),
  chatAsk: (id: string, question: string) => invoke<ChatAnswer>("chat_ask", { id, question }),
  chatExamples: (id: string) => invoke<string[]>("chat_examples", { id }),
  getChatMe: (id: string) => invoke<string | null>("get_chat_me", { id }),
  setChatMe: (id: string, me: string | null) => invoke<void>("set_chat_me", { id, me }),
  llmStatus: () => invoke<LlmStatus>("llm_status"),
  locateDate: (id: string, date: string) => invoke<number | null>("locate_date", { id, date }),
};
