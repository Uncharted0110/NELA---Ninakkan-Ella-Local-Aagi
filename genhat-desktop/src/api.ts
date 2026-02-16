import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import type { ChatMessage, ModelFile } from "./types";

const LLM_API_URL = "http://127.0.0.1:8081/v1/chat/completions";

export const Api = {
  async listModels(): Promise<ModelFile[]> {
    return invoke<ModelFile[]>("list_models");
  },

  async listAudioModels(): Promise<ModelFile[]> {
    return invoke<ModelFile[]>("list_audio_models");
  },

  async switchModel(modelPath: string): Promise<void> {
    return invoke("switch_model", { modelPath });
  },

  async generateSpeech(modelPath: string, input: string): Promise<string> {
    const filePath = await invoke<string>("generate_speech", {
      modelPath,
      input,
    });
    return convertFileSrc(filePath);
  },

  async streamChat(
    messages: ChatMessage[],
    onChunk: (chunk: string) => void,
    onFinish: () => void,
    onError: (err: unknown) => void
  ) {
    try {
      const res = await fetch(LLM_API_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          messages,
          stream: true,
          // reliable defaults
          max_tokens: 1024, 
          temperature: 0.7,
        }),
      });

      if (!res.body) throw new Error("No response body received from local LLM");

      const reader = res.body.getReader();
      const decoder = new TextDecoder("utf-8");
      let buffer = "";

      while (true) {
        const { value, done } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });
        const lines = buffer.split("\n");
        buffer = lines.pop() || ""; // Keep incomplete line in buffer

        for (const line of lines) {
          if (!line.startsWith("data: ")) continue;
          
          const payload = line.replace("data: ", "").trim();
          if (payload === "[DONE]") {
            onFinish();
            return;
          }

          try {
            const parsed = JSON.parse(payload);
            const content = parsed.choices?.[0]?.delta?.content;
            if (content) {
              onChunk(content);
            }
          } catch (e) {
            console.warn("Failed to parse SSE JSON chunk", e);
          }
        }
      }
      
      onFinish(); // Ensure finish is called if stream ends naturally
    } catch (err) {
      onError(err);
    }
  },
};
