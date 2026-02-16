import React, { useState, useEffect, useRef } from "react";

interface ChatWindowProps {
  messages: { role: string; content: string }[];
  streamingContent: string;
  isLoading: boolean;
  onSend: (text: string) => void;
  audioSrc?: string;
}

const ChatWindow: React.FC<ChatWindowProps> = ({
  messages,
  streamingContent,
  isLoading,
  onSend,
  audioSrc,
}) => {
  const [inputObj, setInputObj] = useState("");
  const endRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    endRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamingContent]);

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
        {messages.map((msg, idx) => (
          <div key={idx} className={`message ${msg.role}`}>
            <div className="avatar">{msg.role === "user" ? "You" : "AI"}</div>
            <div className="content">{msg.content}</div>
          </div>
        ))}

        {isLoading && (
          <div className="message assistant loading">
            <div className="avatar">AI</div>
            <div className="content">
              {streamingContent || <span className="typing-indicator">...</span>}
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
        <textarea
          value={inputObj}
          onChange={(e) => setInputObj(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Type a message..."
          rows={1}
        />
        <button onClick={handleSend} disabled={isLoading || !inputObj.trim()}>
          Send
        </button>
      </div>
    </div>
  );
};

export default ChatWindow;
