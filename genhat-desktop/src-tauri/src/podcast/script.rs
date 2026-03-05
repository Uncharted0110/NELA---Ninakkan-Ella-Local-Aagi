//! Script generation — builds LLM prompt and parses dialogue JSON.

use crate::podcast::types::PodcastLine;

/// Build a single prompt string for the LLM to generate a podcast script.
///
/// Returns a single string (not messages array) because the llama-server
/// backend wraps the input in its own system/user message structure.
/// We embed all instructions directly so the LLM gets a clear, self-contained
/// prompt that asks for JSON output.
pub fn build_script_prompt(
    query: &str,
    rag_context: &str,
    speaker_a: &str,
    speaker_b: &str,
    max_turns: usize,
) -> String {
    format!(
        r#"Write a podcast dialogue between two hosts: {speaker_a} and {speaker_b}.

{speaker_a} is the curious interviewer. {speaker_b} is the knowledgeable expert.
Write exactly {max_turns} lines, alternating between {speaker_a} and {speaker_b}.
Base ALL content on the CONTEXT below. Do not invent facts.
Keep each line conversational, 1-3 sentences.
Start with {speaker_a} introducing the topic. End with {speaker_b} wrapping up.

CONTEXT:
{rag_context}

TOPIC: {query}

Respond with ONLY a JSON array, no other text:
[{{"speaker":"{speaker_a}","text":"..."}},{{"speaker":"{speaker_b}","text":"..."}}]"#
    )
}

/// Parse the raw LLM response into structured `PodcastLine` entries.
///
/// Handles common LLM output quirks — markdown-wrapped JSON, trailing text, etc.
pub fn parse_script_response(
    raw_response: &str,
    speaker_a: &str,
    speaker_b: &str,
    voice_a: &str,
    voice_b: &str,
) -> Result<Vec<PodcastLine>, String> {
    log::info!("[podcast] Raw LLM script output ({} chars): {}",
        raw_response.len(),
        &raw_response[..raw_response.len().min(500)]
    );

    let json_str = extract_json_array(raw_response)?;
    log::debug!("[podcast] Extracted JSON ({} chars): {}",
        json_str.len(),
        &json_str[..json_str.len().min(300)]
    );

    let parsed: Vec<serde_json::Value> = serde_json::from_str(&json_str)
        .map_err(|e| format!("Failed to parse script JSON: {e}\nExtracted: {}",
            &json_str[..json_str.len().min(200)]
        ))?;

    if parsed.is_empty() {
        return Err("LLM returned an empty script".to_string());
    }

    let mut lines = Vec::new();
    for (i, entry) in parsed.iter().enumerate() {
        let speaker = entry["speaker"]
            .as_str()
            .ok_or_else(|| format!("Missing 'speaker' at line {i}"))?
            .to_string();

        let text = entry["text"]
            .as_str()
            .ok_or_else(|| format!("Missing 'text' at line {i}"))?
            .to_string();

        // Map speaker name → voice
        let voice = if speaker == speaker_a {
            voice_a.to_string()
        } else if speaker == speaker_b {
            voice_b.to_string()
        } else {
            // Unknown speaker — default to voice_a
            log::warn!("Unknown speaker '{}' at line {}, defaulting to voice_a", speaker, i);
            voice_a.to_string()
        };

        lines.push(PodcastLine {
            speaker,
            voice,
            text,
            index: i,
        });
    }

    Ok(lines)
}

/// Extract a JSON array from potentially wrapped LLM output.
///
/// Handles:
/// - Raw JSON: `[{...}, ...]`
/// - Markdown fences: ` ```json\n[...]\n``` `
/// - Prefix/suffix text around the array
/// - Nested brackets (finds the outermost balanced pair)
fn extract_json_array(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();

    if trimmed.is_empty() {
        return Err("LLM returned empty response".to_string());
    }

    // Find the first '[' character
    let start = match trimmed.find('[') {
        Some(pos) => pos,
        None => return Err(format!(
            "No JSON array found in LLM response (first 200 chars): {}",
            &trimmed[..trimmed.len().min(200)]
        )),
    };

    // Find the matching closing bracket by tracking bracket depth
    let mut depth = 0;
    let mut end_pos = None;
    for (i, ch) in trimmed[start..].char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    end_pos = Some(start + i);
                    break;
                }
            }
            _ => {}
        }
    }

    match end_pos {
        Some(end) => Ok(trimmed[start..=end].to_string()),
        None => {
            // Fallback: try rfind for the last ']'
            if let Some(end) = trimmed.rfind(']') {
                if end > start {
                    return Ok(trimmed[start..=end].to_string());
                }
            }
            Err(format!(
                "No closing bracket found in LLM response (first 200 chars): {}",
                &trimmed[..trimmed.len().min(200)]
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_array_raw() {
        let input = r#"[{"speaker":"A","text":"Hello"},{"speaker":"B","text":"Hi"}]"#;
        let result = extract_json_array(input).unwrap();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn test_extract_json_array_with_markdown() {
        let input = "Here is the script:\n```json\n[{\"speaker\":\"A\",\"text\":\"Hello\"}]\n```\nDone.";
        let result = extract_json_array(input).unwrap();
        assert!(result.starts_with('['));
        assert!(result.ends_with(']'));
    }

    #[test]
    fn test_parse_script_response() {
        let raw = r#"[{"speaker":"Alex","text":"Welcome!"},{"speaker":"Sam","text":"Thanks for having me."}]"#;
        let lines = parse_script_response(raw, "Alex", "Sam", "Leo", "Bella").unwrap();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].speaker, "Alex");
        assert_eq!(lines[0].voice, "Leo");
        assert_eq!(lines[1].speaker, "Sam");
        assert_eq!(lines[1].voice, "Bella");
    }
}
