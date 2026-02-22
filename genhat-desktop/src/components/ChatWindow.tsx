import React, { useState, useEffect, useRef } from "react";
import { User, Bot, SendHorizonal, Sparkles } from "lucide-react";
import MarkdownRenderer from "./MarkdownRenderer";

interface ChatWindowProps {
  messages: { role: string; content: string }[];
  streamingContent: string;
  isLoading: boolean;
  onSend: (text: string) => void;
  audioSrc?: string;
  placeholder?: string;
}

const ChatWindow: React.FC<ChatWindowProps> = ({
  messages,
  streamingContent,
  isLoading,
  onSend,
  audioSrc,
  placeholder = "Message GenHat...",
}) => {
  const [inputObj, setInputObj] = useState("");
  const endRef = useRef<HTMLDivElement>(null);
  const textareaRef = useRef<HTMLTextAreaElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingContent]);

  // Auto-resize textarea
  useEffect(() => {
    const ta = textareaRef.current;
    if (ta) {
      ta.style.height = "auto";
      ta.style.height = Math.min(ta.scrollHeight, 200) + "px";
    }
  }, [inputObj]);

  const handleSend = () => {
    if (!inputObj.trim()) return;
    onSend(inputObj);
    setInputObj("");
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };

  return (
    <div className="chat-container">
      <div className="messages-area">
        {messages.length === 0 && !isLoading && (
          <div className="empty-state">
            <div className="empty-icon">
              <Sparkles size={48} strokeWidth={1.2} />
            </div>
            <h2>GenHat</h2>
            <p>Your local intelligence engine. Start a conversation below.</p>
          </div>
        )}

        {messages.map((msg, idx) => (
          <div key={idx} className={`message ${msg.role}`}>
            <div className="avatar">
              {msg.role === "user" ? (
                <User size={18} strokeWidth={2} />
              ) : (
                <Bot size={18} strokeWidth={2} />
              )}
            </div>
            <div className="msg-body">
              <span className="msg-role">{msg.role === "user" ? "You" : "GenHat"}</span>
              <div className="content">
                {msg.role === "assistant" ? (
                  <MarkdownRenderer content={msg.content} />
                ) : (
                  msg.content
                )}
              </div>
            </div>
          </div>
        ))}

        {isLoading && (
          <div className="message assistant loading">
            <div className="avatar">
              <Bot size={18} strokeWidth={2} />
            </div>
            <div className="msg-body">
              <span className="msg-role">GenHat</span>
              <div className="content">
                {streamingContent ? (
                  <MarkdownRenderer content={streamingContent} />
                ) : (
                  <div className="typing-dots">
                    <span></span><span></span><span></span>
                  </div>
                )}
              </div>
            </div>
          </div>
        )}

        {/* Audio Player if generated */}
        {audioSrc && (
          <div className="audio-player">
            <audio controls src={audioSrc} autoPlay />
          </div>
        )}

        <div ref={endRef} />
      </div>

      <div className="input-area">
        <div className="input-wrapper">
          <textarea
            ref={textareaRef}
            value={inputObj}
            onChange={(e) => setInputObj(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={placeholder}
            rows={1}
          />
          <button
            className="send-btn"
            onClick={handleSend}
            disabled={isLoading || !inputObj.trim()}
            title="Send message"
          >
            <SendHorizonal size={18} strokeWidth={2} />
          </button>
        </div>
        <span className="input-hint">
          Press Enter to send, Shift+Enter for new line
        </span>
      </div>
    </div>
  );
};

export default ChatWindow;
