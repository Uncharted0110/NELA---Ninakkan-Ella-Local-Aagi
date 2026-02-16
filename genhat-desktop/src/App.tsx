import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Api } from "./api";
import type { ChatMessage, ModelFile } from "./types";
import Sidebar from "./components/Sidebar";
import ChatWindow from "./components/ChatWindow";
import "./App.css";

function App() {
  const [models, setModels] = useState<ModelFile[]>([]);
  const [selectedModel, setSelectedModel] = useState<string>("");
  
  const [audioModels, setAudioModels] = useState<ModelFile[]>([]);
  const [selectedAudioModel, setSelectedAudioModel] = useState<string>("None");
  const [audioOutput, setAudioOutput] = useState<string>("");

  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [streamingContent, setStreamingContent] = useState<string>("");
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    Api.listModels()
      .then((list) => {
        setModels(list);
        if (list.length > 0) {
          setSelectedModel(list[0].path);
        }
      })
      .catch(console.error);

    Api.listAudioModels()
      .then((list) => {
        setAudioModels(list);
      })
      .catch(console.error);
  }, []);

  const handleModelChange = async (path: string) => {
    try {
      setSelectedModel(path);
      await Api.switchModel(path);
      setMessages([]); // Clear chat on model switch
      alert(`Switched to model: ${path.split("/").pop()}`);
    } catch (err) {
      console.error(err);
      alert("Failed to switch model");
    }
  };

  const handleSend = async (text: string) => {
    const newMsg: ChatMessage = { role: "user", content: text };
    setMessages((prev) => [...prev, newMsg]);
    setLoading(true);
    setStreamingContent("");
    setAudioOutput("");

    // If Audio Mode is enabled
    if (selectedAudioModel && selectedAudioModel !== "None") {
      try {
        // For audio, we just generate speech from the prompt directly (demo mode)
        // Ideally we would get the LLM response first, then TTS that.
        // But following existing logic: user prompt -> audio.
        // Wait, the original code did `input: prompt`.
        
        // Let's improve: LLM -> TTS
        // 1. Get LLM response
        let fullResponse = "";
        await Api.streamChat(
          [...messages, newMsg],
          (chunk) => {
            setStreamingContent((prev) => prev + chunk);
            fullResponse += chunk;
          },
          async () => {
             // 2. Generate Audio from LLM response
             try {
                const audioPath = await Api.generateSpeech(selectedAudioModel, fullResponse);
                setAudioOutput(audioPath);
             } catch (e) {
                console.error("TTS Error:", e);
             }
             setLoading(false);
             setMessages(prev => [...prev, { role: "assistant", content: fullResponse }]);
             setStreamingContent("");
          },
          (err) => {
             console.error(err);
             setLoading(false);
          }
        );

      } catch (e) {
        console.error(e);
        setLoading(false);
      }
      return;
    }

    // Normal Text Chat
    let fullResponse = "";
    Api.streamChat(
      [...messages, newMsg],
      (chunk) => {
        setStreamingContent((prev) => prev + chunk);
        fullResponse += chunk;
      },
      () => {
        setLoading(false);
        if (fullResponse) {
          setMessages((prev) => [...prev, { role: "assistant", content: fullResponse }]);
          setStreamingContent("");
        }
      },
      (err) => {
        console.error("Stream error", err);
        setLoading(false);
      }
    );
  };

  return (
    <div className="app-container">
      <Sidebar
        models={models}
        selectedModel={selectedModel}
        onModelSelect={handleModelChange}
        audioModels={audioModels}
        selectedAudio={selectedAudioModel}
        onAudioSelect={setSelectedAudioModel}
      />
      <main className="main-content">
        <ChatWindow 
           messages={messages}
           streamingContent={streamingContent}
           isLoading={loading}
           onSend={handleSend}
           audioSrc={audioOutput}
        />
      </main>
    </div>
  );
}

export default App;
