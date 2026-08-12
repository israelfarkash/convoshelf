import { useState, useEffect, useRef } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { Virtuoso } from "react-virtuoso";
import type { VirtuosoHandle } from "react-virtuoso";
import { MediaDisplay } from "./MediaDisplay";
import "./index.css";

// ─── Types ───────────────────────────────────────────────────────
interface Chat {
  id: string;
  name: string;
  created_at: string;
  message_count: number;
}

interface Message {
  id: string;
  chat_id: string;
  timestamp: string;
  sender: string;
  msg_type: string;
  text: string | null;
  original_text: string | null;
  media_path: string | null;
  media_filename: string | null;
  media_mime_type: string | null;
  edited: boolean;
  deleted: boolean;
  system: boolean;
}

interface ChatStats {
  total: number;
  images: number;
  videos: number;
  audios: number;
  documents: number;
  stickers: number;
  deleted: number;
}

// ─── Helpers ─────────────────────────────────────────────────────
const SENDER_COLORS = [
  "#e15d5d", "#d4a037", "#6bb34c", "#3fa0e0",
  "#ca6dca", "#e07e53", "#53bfbf", "#7d7de0",
];

function senderColor(sender: string): string {
  let hash = 0;
  for (let i = 0; i < sender.length; i++) {
    hash = sender.charCodeAt(i) + ((hash << 5) - hash);
  }
  return SENDER_COLORS[Math.abs(hash) % SENDER_COLORS.length];
}

function getInitials(name: string): string {
  const parts = name.split(/\s+/).filter(Boolean);
  if (parts.length === 0) return "?";
  if (parts.length === 1) return parts[0].charAt(0);
  return parts[0].charAt(0) + parts[parts.length - 1].charAt(0);
}

function formatDate(timestamp: string): string {
  // timestamp format: "dd.mm.yyyy, hh:mm:ss" or similar
  const match = timestamp.match(/(\d{1,2})[./](\d{1,2})[./](\d{2,4})/);
  if (!match) return timestamp;
  const day = parseInt(match[1]);
  const month = parseInt(match[2]);
  const year = parseInt(match[3]);
  
  const today = new Date();
  const msgDate = new Date(year, month - 1, day);
  
  const diffTime = today.getTime() - msgDate.getTime();
  const diffDays = Math.floor(diffTime / (1000 * 60 * 60 * 24));
  
  if (diffDays === 0) return "היום";
  if (diffDays === 1) return "אתמול";
  
  const months = ["ינואר", "פברואר", "מרץ", "אפריל", "מאי", "יוני", "יולי", "אוגוסט", "ספטמבר", "אוקטובר", "נובמבר", "דצמבר"];
  return `${day} ב${months[month - 1]} ${year}`;
}

function extractTime(timestamp: string): string {
  const match = timestamp.match(/(\d{1,2}:\d{2}(:\d{2})?)/);
  return match ? match[1] : "";
}

function getDateKey(timestamp: string): string {
  const match = timestamp.match(/(\d{1,2}[./]\d{1,2}[./]\d{2,4})/);
  return match ? match[1] : "";
}

function isMediaFile(path: string): "image" | "video" | "sticker" | "audio" | "document" | null {
  const lower = path.toLowerCase();
  if (lower.match(/\.(jpg|jpeg|png|gif)$/)) return "image";
  if (lower.match(/\.webp$/) && lower.includes("sticker")) return "sticker";
  if (lower.match(/\.webp$/)) return "image";
  if (lower.match(/\.(mp4|mov)$/)) return "video";
  if (lower.match(/\.(mp3|m4a|ogg|opus)$/)) return "audio";
  return "document";
}

// ─── App Component ───────────────────────────────────────────────
function App() {
  const [chats, setChats] = useState<Chat[]>([]);
  const [activeChat, setActiveChat] = useState<Chat | null>(null);
  const [messages, setMessages] = useState<Message[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadingText, setLoadingText] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchIndices, setSearchIndices] = useState<number[]>([]);
  const [currentSearchIndex, setCurrentSearchIndex] = useState(-1);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; msg: Message } | null>(null);
  const [editingMsg, setEditingMsg] = useState<Message | null>(null);
  const [editText, setEditText] = useState("");
  const [lightboxSrc, setLightboxSrc] = useState<string | null>(null);
  const [editingChatName, setEditingChatName] = useState(false);
  const [newChatName, setNewChatName] = useState("");
  const [importProgress, setImportProgress] = useState(0);
  const [infoOpen, setInfoOpen] = useState(false);
  const [chatStats, setChatStats] = useState<ChatStats | null>(null);
  
  const virtuosoRef = useRef<VirtuosoHandle>(null);

  // ── Load & Listen ──
  useEffect(() => {
    loadChats();

    const unlistenDrop = listen<{ paths: string[] }>("tauri://drag-drop", async (event) => {
      const paths = event.payload.paths;
      if (paths && paths.length > 0) {
        for (const p of paths) {
          if (p.endsWith(".zip")) {
            await handleImport(p);
          }
        }
      }
    });

    const unlistenProgress = listen<{ step: string; progress: number }>("import-progress", (event) => {
      setLoadingText(event.payload.step);
      setImportProgress(event.payload.progress);
    });

    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && (e.key === "f" || e.key === "k")) {
        e.preventDefault();
        setSearchOpen(prev => !prev);
      }
      if (e.key === "Escape") {
        setSearchOpen(false);
        setSearchQuery("");
        setContextMenu(null);
        setLightboxSrc(null);
        setEditingMsg(null);
      }
    };
    window.addEventListener("keydown", handleKeyDown);

    return () => {
      unlistenDrop.then(f => f());
      unlistenProgress.then(f => f());
      window.removeEventListener("keydown", handleKeyDown);
    };
  }, []);

  // ── Close context menu on click ──
  useEffect(() => {
    const close = () => setContextMenu(null);
    window.addEventListener("click", close);
    return () => window.removeEventListener("click", close);
  }, []);

  // ── Scroll to bottom on new messages ──
  useEffect(() => {
    if (virtuosoRef.current && messages.length > 0 && !searchOpen) {
      virtuosoRef.current.scrollToIndex({ index: messages.length - 1, align: 'end' });
    }
  }, [messages]);

  // ── Search Navigation ──
  useEffect(() => {
    if (searchQuery.trim()) {
      const q = searchQuery.toLowerCase();
      const indices: number[] = [];
      messages.forEach((m, i) => {
        if (m.text?.toLowerCase().includes(q) || m.sender?.toLowerCase().includes(q)) {
          indices.push(i);
        }
      });
      setSearchIndices(indices);
      if (indices.length > 0) {
        setCurrentSearchIndex(indices.length - 1); // Start at the newest matching message
      } else {
        setCurrentSearchIndex(-1);
      }
    } else {
      setSearchIndices([]);
      setCurrentSearchIndex(-1);
    }
  }, [searchQuery, messages]);

  useEffect(() => {
    if (currentSearchIndex >= 0 && searchIndices.length > 0) {
      const msgIndex = searchIndices[currentSearchIndex];
      virtuosoRef.current?.scrollToIndex({ index: msgIndex, align: 'center', behavior: 'smooth' });
    }
  }, [currentSearchIndex]);

  // ── Handlers ──
  const scrollToTop = () => {
    if (virtuosoRef.current) {
      virtuosoRef.current.scrollToIndex({ index: 0, align: 'start', behavior: 'smooth' });
    }
  };

  const scrollToBottom = () => {
    if (virtuosoRef.current && messages.length > 0) {
      virtuosoRef.current.scrollToIndex({ index: messages.length - 1, align: 'end', behavior: 'smooth' });
    }
  };

  async function handleImport(path: string) {
    setLoading(true);
    setLoadingText("מתחיל ייבוא...");
    setImportProgress(0);
    try {
      const newChatId = await invoke<string>("import_zip", { zipPath: path });
      setLoadingText("טוען שיחות...");
      await loadChats();
      await loadMessages(newChatId);
    } catch (e) {
      alert("שגיאה בייבוא: " + e);
    }
    setLoading(false);
    setImportProgress(0);
  }

  async function openDialog() {
    const selected = await open({
      multiple: false,
      filters: [{ name: "WhatsApp Export", extensions: ["zip"] }],
    });
    if (selected) {
      const path = typeof selected === "string" ? selected : (selected as any).path;
      if (path) handleImport(path);
    }
  }

  async function loadChats() {
    try {
      const data: Chat[] = await invoke("get_chats");
      setChats(data);
      // If only one chat and none active, auto-select it
      if (data.length === 1 && !activeChat) {
        setActiveChat(data[0]);
        loadMessages(data[0].id);
      }
    } catch (e) {
      console.error(e);
    }
  }

  async function loadMessages(chatId: string) {
    try {
      let data: Message[];
      if (searchQuery) {
        data = await invoke<Message[]>("search_messages", { chatId, query: searchQuery });
      } else {
        data = await invoke<Message[]>("get_all_messages", { chatId });
      }
      setMessages(data);
      const stats = await invoke<ChatStats>("get_chat_stats", { chatId });
      setChatStats(stats);
    } catch (e) {
      console.error(e);
    }
  }

  async function handleEdit() {
    if (!editingMsg) return;
    try {
      await invoke("edit_message", { msgId: editingMsg.id, newText: editText });
      setMessages(prev =>
        prev.map(m => m.id === editingMsg.id ? { ...m, text: editText, edited: true } : m)
      );
    } catch (e) {
      console.error(e);
    }
    setEditingMsg(null);
  }

  async function handleDelete(msg: Message) {
    try {
      await invoke("delete_message", { msgId: msg.id });
      setMessages(prev =>
        prev.map(m => m.id === msg.id ? { ...m, deleted: true } : m)
      );
    } catch (e) {
      console.error(e);
    }
  }

  async function handleRenameChat() {
    if (!activeChat || !newChatName.trim()) return;
    try {
      await invoke("rename_chat", { chatId: activeChat.id, newName: newChatName.trim() });
      const name = newChatName.trim();
      setChats(prev => prev.map(c => c.id === activeChat.id ? { ...c, name } : c));
      setActiveChat(prev => prev ? { ...prev, name } : null);
    } catch (e) {
      console.error(e);
    }
    setEditingChatName(false);
  }

  async function handleRestore(msg: Message) {
    try {
      await invoke("restore_message", { msgId: msg.id });
      setMessages(prev =>
        prev.map(m => m.id === msg.id ? { ...m, deleted: false, text: m.original_text, edited: false } : m)
      );
    } catch (e) {
      console.error(e);
    }
  }

  // ── Render ──
  return (
    <div className="app-container">
      {/* Loading overlay */}
      {loading && (
        <div className="loading-overlay">
          <div className="loading-spinner" />
          <div className="loading-text">{loadingText}</div>
        </div>
      )}

      {/* Lightbox */}
      {lightboxSrc && (
        <div className="lightbox-overlay" onClick={() => setLightboxSrc(null)}>
          <button className="lightbox-close" onClick={() => setLightboxSrc(null)}>✕</button>
          <img src={lightboxSrc} className="lightbox-img" alt="" onClick={e => e.stopPropagation()} />
        </div>
      )}

      {/* Edit Modal */}
      {editingMsg && (
        <div className="edit-overlay" onClick={() => setEditingMsg(null)}>
          <div className="edit-modal" onClick={e => e.stopPropagation()}>
            <h3>עריכת הודעה</h3>
            <textarea
              value={editText}
              onChange={e => setEditText(e.target.value)}
              autoFocus
            />
            <div className="edit-modal-actions">
              <button className="edit-btn-save" onClick={handleEdit}>שמור</button>
              <button className="edit-btn-cancel" onClick={() => setEditingMsg(null)}>ביטול</button>
            </div>
          </div>
        </div>
      )}

      {/* Context Menu */}
      {contextMenu && (
        <div
          className="context-menu"
          style={{ top: contextMenu.y, left: contextMenu.x }}
          onClick={e => e.stopPropagation()}
        >
          <div className="context-menu-item" onClick={() => {
            if (contextMenu.msg.text) navigator.clipboard.writeText(contextMenu.msg.text);
            setContextMenu(null);
          }}>
            📋 העתק
          </div>
          {!contextMenu.msg.system && !contextMenu.msg.deleted && (
            <div className="context-menu-item" onClick={() => {
              setEditingMsg(contextMenu.msg);
              setEditText(contextMenu.msg.text || "");
              setContextMenu(null);
            }}>
              ✏️ ערוך
            </div>
          )}
          {!contextMenu.msg.deleted ? (
            <div className="context-menu-item danger" onClick={() => {
              handleDelete(contextMenu.msg);
              setContextMenu(null);
            }}>
              🗑️ מחק
            </div>
          ) : (
            <div className="context-menu-item" onClick={() => {
              handleRestore(contextMenu.msg);
              setContextMenu(null);
            }}>
              ↩️ שחזר
            </div>
          )}
          {contextMenu.msg.edited && contextMenu.msg.original_text && (
            <>
              <div className="context-menu-divider" />
              <div className="context-menu-item" onClick={() => {
                alert("טקסט מקורי:\n\n" + contextMenu.msg.original_text);
                setContextMenu(null);
              }}>
                📄 הצג מקור
              </div>
            </>
          )}
        </div>
      )}

      {/* ─── Sidebar ─── */}
      <div className="sidebar">
        <div className="sidebar-header">
          <div style={{ width: 40, height: 40, borderRadius: '50%', background: '#dfe5e7', display: 'flex', alignItems: 'center', justifyContent: 'center', fontSize: 20 }}>
            💬
          </div>
          <h1>WhatsApp Export Viewer</h1>
        </div>
        <div className="sidebar-search">
          <input
            placeholder="חיפוש שיחה..."
            onChange={() => {/* sidebar search filter */}}
          />
        </div>
        <div className="chat-list">
          {chats.map(chat => (
            <div
              key={chat.id}
              className={`chat-item ${activeChat?.id === chat.id ? "active" : ""}`}
              onClick={() => { setActiveChat(chat); loadMessages(chat.id); }}
            >
              <div className="chat-avatar" style={{ background: senderColor(chat.name) }}>
                {getInitials(chat.name)}
              </div>
              <div className="chat-info">
                <div className="chat-info-top">
                  <span className="chat-name">{chat.name}</span>
                  <span className="chat-time">{new Date(chat.created_at).toLocaleDateString("he-IL")}</span>
                </div>
                <div className="chat-last-msg">
                  {chat.message_count > 0 ? `${chat.message_count.toLocaleString()} הודעות` : "שיחה ריקה"}
                </div>
              </div>
            </div>
          ))}
          {chats.length === 0 && (
            <div style={{ padding: 32, textAlign: 'center', color: '#667781' }}>
              <div style={{ fontSize: 48, marginBottom: 12 }}>📱</div>
              <p>אין שיחות עדיין</p>
              <p style={{ fontSize: 13, marginTop: 4 }}>ייבא קובץ ZIP של WhatsApp</p>
            </div>
          )}
        </div>
        <div style={{ padding: 12, borderTop: '1px solid var(--wa-border)' }}>
          <button className="import-btn" style={{ width: '100%' }} onClick={openDialog} disabled={loading}>
            {loading ? "מייבא..." : "➕ ייבוא שיחה חדשה"}
          </button>
        </div>
      </div>

      {/* ─── Main Chat ─── */}
      <div className="main-area" style={{ flexDirection: 'row' }}>
        {activeChat ? (
          <>
            <div style={{ flex: 1, display: 'flex', flexDirection: 'column' }}>
              {/* Header */}
            <div className="chat-header">
              <div className="chat-avatar" style={{ background: senderColor(activeChat.name) }}>
                {getInitials(activeChat.name)}
              </div>
              <div className="chat-header-info">
                {editingChatName ? (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 6 }}>
                    <input
                      value={newChatName}
                      onChange={e => setNewChatName(e.target.value)}
                      onKeyDown={e => { if (e.key === "Enter") handleRenameChat(); if (e.key === "Escape") setEditingChatName(false); }}
                      autoFocus
                      style={{
                        fontSize: 15,
                        padding: '4px 8px',
                        borderRadius: 6,
                        border: '1px solid var(--wa-teal)',
                        outline: 'none',
                        background: 'var(--wa-bg-incoming)',
                        color: 'var(--wa-text-primary)'
                      }}
                    />
                    <button className="import-btn" style={{ padding: '4px 10px', fontSize: 13 }} onClick={handleRenameChat}>שמור</button>
                    <button className="search-close-btn" onClick={() => setEditingChatName(false)}>✕</button>
                  </div>
                ) : (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                    <div className="chat-header-name">{activeChat.name}</div>
                    <button
                      title="שנה שם שיחה"
                      style={{ background: 'none', border: 'none', cursor: 'pointer', fontSize: 14, opacity: 0.6 }}
                      onClick={() => { setNewChatName(activeChat.name); setEditingChatName(true); }}
                    >
                      ✏️
                    </button>
                  </div>
                )}
                <div className="chat-header-status">
                  {messages.length.toLocaleString()} הודעות
                </div>
              </div>
              <div className="chat-header-actions">
                <button className="header-btn" onClick={() => setSearchOpen(!searchOpen)} title="חיפוש (⌘F)">🔍</button>
                <button className="header-btn" onClick={() => setInfoOpen(!infoOpen)} title="פרטי הקבוצה">ℹ️</button>
              </div>
            </div>

            {/* Search Panel */}
            {searchOpen && (
              <div className="search-panel" style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '10px 16px' }}>
                <input
                  style={{ flex: 1, padding: '6px 12px', borderRadius: 16, border: 'none', outline: 'none', background: 'var(--wa-bg-panel)', color: 'var(--wa-text-primary)' }}
                  placeholder="חיפוש הודעות..."
                  value={searchQuery}
                  onChange={e => setSearchQuery(e.target.value)}
                  autoFocus
                />
                {searchIndices.length > 0 && (
                  <div style={{ display: 'flex', alignItems: 'center', gap: 8, color: '#8696a0', fontSize: 14 }}>
                    <span>{currentSearchIndex + 1} / {searchIndices.length} תוצאות</span>
                    <button 
                      className="header-btn" 
                      onClick={() => setCurrentSearchIndex(prev => Math.min(prev + 1, searchIndices.length - 1))}
                      disabled={currentSearchIndex >= searchIndices.length - 1}
                    >⬇️</button>
                    <button 
                      className="header-btn" 
                      onClick={() => setCurrentSearchIndex(prev => Math.max(prev - 1, 0))}
                      disabled={currentSearchIndex <= 0}
                    >⬆️</button>
                  </div>
                )}
                {searchQuery && searchIndices.length === 0 && (
                  <span style={{ color: '#8696a0', fontSize: 14 }}>אין תוצאות</span>
                )}
                <button className="search-close-btn" onClick={() => { setSearchOpen(false); setSearchQuery(""); }}>✕</button>
              </div>
            )}

            {/* Messages */}
            <div className="messages-container">
              
              <button className="scroll-btn top" onClick={scrollToTop} title="לראש השיחה">⬆️</button>
              <button className="scroll-btn bottom" onClick={scrollToBottom} title="לסוף השיחה">⬇️</button>

              <Virtuoso
                ref={virtuosoRef}
                data={messages}
                initialTopMostItemIndex={messages.length - 1}
                itemContent={(i, msg) => {
                  const prevMsg = i > 0 ? messages[i - 1] : null;
                  const prevDateKey = prevMsg ? getDateKey(prevMsg.timestamp) : null;
                  const currDateKey = getDateKey(msg.timestamp);
                  const showDateSep = currDateKey !== prevDateKey;

                  const sameSenderAsPrev = prevMsg && !prevMsg.system && !msg.system && prevMsg.sender === msg.sender && !showDateSep;
                  const isFirstInGroup = !sameSenderAsPrev;
                  
                  const isHighlighted = searchIndices.length > 0 && searchIndices[currentSearchIndex] === i;

                  // System message
                  if (msg.system || msg.msg_type === "system") {
                    return (
                      <div key={msg.id} style={{ paddingBottom: 2 }}>
                        {showDateSep && (
                          <div className="date-separator">
                            <div className="date-separator-pill">{formatDate(msg.timestamp)}</div>
                          </div>
                        )}
                        <div className="system-message">
                          <div className="system-message-pill">{msg.text}</div>
                        </div>
                      </div>
                    );
                  }

                  const mediaType = msg.media_path ? isMediaFile(msg.media_path) : null;

                  return (
                    <div key={msg.id} style={{ paddingBottom: 2 }}>
                      {showDateSep && (
                        <div className="date-separator">
                          <div className="date-separator-pill">{formatDate(msg.timestamp)}</div>
                        </div>
                      )}
                      <div
                        className={`message-row incoming ${isFirstInGroup ? "first-in-group" : ""}`}
                        onContextMenu={e => {
                          e.preventDefault();
                          setContextMenu({ x: e.clientX, y: e.clientY, msg });
                        }}
                      >
                        <div className={`message-bubble incoming ${isFirstInGroup ? "tail" : ""}`} style={isHighlighted ? { backgroundColor: 'var(--wa-teal)', color: '#fff' } : {}}>
                          {/* Sender name */}
                          {isFirstInGroup && (
                            <div className="message-sender" style={{ color: senderColor(msg.sender) }}>
                              {msg.sender}
                            </div>
                          )}

                          {/* Deleted message */}
                          {msg.deleted ? (
                            <div className="deleted-message-text">
                              <span className="deleted-icon">🚫</span>
                              הודעה זו נמחקה
                            </div>
                          ) : (
                            <>
                              {/* Media */}
                              {msg.msg_type === "media_omitted" && (
                                <div className="media-omitted">📷 מדיה הושמטה</div>
                              )}

                              {msg.media_path && (mediaType === "image" || mediaType === "sticker") && (
                                <div className="media-container">
                                  <MediaDisplay
                                    path={msg.media_path}
                                    type={mediaType}
                                    onClick={(src) => setLightboxSrc(src)}
                                  />
                                </div>
                              )}

                              {msg.media_path && mediaType === "video" && (
                                <div className="media-container">
                                  <MediaDisplay path={msg.media_path} type="video" />
                                </div>
                              )}

                              {msg.media_path && mediaType === "audio" && (
                                <div style={{ padding: '4px 0' }}>
                                  <MediaDisplay path={msg.media_path} type="audio" />
                                </div>
                              )}

                              {msg.media_path && mediaType === "document" && (
                                <div className="media-document">
                                  <div className="media-document-icon">
                                    {(msg.media_filename || msg.media_path).split('.').pop()?.toUpperCase().slice(0, 3) || "DOC"}
                                  </div>
                                  <div className="media-document-info">
                                    <div className="media-document-name">
                                      {msg.media_filename || msg.media_path.split('/').pop()}
                                    </div>
                                    <div className="media-document-size">מסמך</div>
                                  </div>
                                </div>
                              )}

                              {/* Text */}
                              {msg.text && msg.msg_type !== "media_omitted" && !msg.text.startsWith("<attached:") && (
                                <div className="message-text">{msg.text}</div>
                              )}
                            </>
                          )}

                          {/* Timestamp */}
                          <div className="message-meta">
                            {msg.edited && <span className="message-edited">נערך</span>}
                            <span className="message-time">{extractTime(msg.timestamp)}</span>
                          </div>
                          <div style={{ clear: 'both' }} />
                        </div>
                      </div>
                    </div>
                  );
                }}
              />
            </div>
          </div>
          
          {/* Chat Info Panel */}
          {infoOpen && chatStats && activeChat && (
            <div className="chat-info-panel" style={{ borderLeft: '1px solid var(--wa-border)', borderRight: 'none' }}>
              <div className="chat-info-header">
                <button className="search-close-btn" onClick={() => setInfoOpen(false)}>✕</button>
                פרטי קבוצה
              </div>
              <div className="chat-info-body">
                <div style={{ textAlign: 'center', marginBottom: 30 }}>
                  <div className="chat-avatar" style={{ width: 120, height: 120, fontSize: 40, margin: '0 auto 10px' }}>
                    {getInitials(activeChat.name)}
                  </div>
                  <h2 style={{ fontSize: 20, margin: 0, color: 'var(--wa-text-primary)' }}>{activeChat.name}</h2>
                  <div style={{ fontSize: 13, color: '#8696a0', marginTop: 5 }}>
                    {chatStats.total.toLocaleString()} הודעות
                  </div>
                </div>

                <div className="chat-info-item">
                  <div className="chat-info-label">תמונות</div>
                  <div className="chat-info-value">{chatStats.images.toLocaleString()}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">סרטונים</div>
                  <div className="chat-info-value">{chatStats.videos.toLocaleString()}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">הקלטות אודיו</div>
                  <div className="chat-info-value">{chatStats.audios.toLocaleString()}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">מסמכים</div>
                  <div className="chat-info-value">{chatStats.documents.toLocaleString()}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">הודעות שנמחקו</div>
                  <div className="chat-info-value">{chatStats.deleted.toLocaleString()}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">תאריך הודעה ראשונה</div>
                  <div className="chat-info-value">{messages.length > 0 ? formatDate(messages[0].timestamp) : '-'}</div>
                </div>
                <div className="chat-info-item">
                  <div className="chat-info-label">תאריך הודעה אחרונה</div>
                  <div className="chat-info-value">{messages.length > 0 ? formatDate(messages[messages.length - 1].timestamp) : '-'}</div>
                </div>
              </div>
            </div>
          )}
          </>
        ) : (
          <div className="empty-state">
            <div style={{ fontSize: 80, opacity: 0.3 }}>💬</div>
            <h2>WhatsApp Export Viewer</h2>
            <p>
              ייבא קובץ ZIP של WhatsApp Export כדי לצפות בשיחה.
              <br />
              גרור קובץ ZIP לכאן או לחץ על הכפתור למטה.
            </p>
            {loading ? (
              <div style={{ width: '100%', maxWidth: 400, marginTop: 20 }}>
                <div style={{ textAlign: 'center', marginBottom: 10, color: 'var(--wa-text-primary)' }}>{loadingText}</div>
                <div style={{ width: '100%', height: 10, background: 'var(--wa-border)', borderRadius: 5, overflow: 'hidden' }}>
                  <div style={{ width: `${importProgress}%`, height: '100%', background: 'var(--wa-teal)', transition: 'width 0.2s' }} />
                </div>
              </div>
            ) : (
              <button className="import-btn" onClick={openDialog}>
                בחר קובץ ZIP
              </button>
            )}
          </div>
        )}
      </div>
    </div>
  );
}

export default App;
