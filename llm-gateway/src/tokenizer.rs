//! Token counting with a real BPE tokenizer.
//!
//! The upstream's own `usage` is always preferred (it is exact and free). This
//! module exists for the fallback path: some providers omit `usage` on streamed
//! responses, and recording 0 tokens makes the console useless. `cl100k_base` is
//! the encoding OpenAI-compatible providers are trained around, so counts land
//! within a few percent for the same text — far better than a chars/4 guess.

use std::sync::OnceLock;
use tiktoken_rs::{cl100k_base, CoreBPE};

fn bpe() -> &'static CoreBPE {
    static BPE: OnceLock<CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| cl100k_base().expect("cl100k_base vocabulary is embedded"))
}

/// Count tokens in `text` with the real BPE encoder.
pub fn count(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    bpe().encode_with_special_tokens(text).len() as u64
}

/// Flatten a chat request payload (OpenAI or Anthropic shape) into the text the
/// model actually reads: system prompt, every message's content, tool schemas and
/// tool-call arguments. Image blocks contribute nothing measurable, so they are
/// skipped rather than guessed.
pub fn payload_text(v: &serde_json::Value) -> String {
    let mut out = String::new();
    if let Some(s) = v.get("system") {
        push_content(s, &mut out);
    }
    if let Some(msgs) = v.get("messages").and_then(|m| m.as_array()) {
        for m in msgs {
            if let Some(role) = m.get("role").and_then(|r| r.as_str()) {
                out.push_str(role);
                out.push('\n');
            }
            if let Some(c) = m.get("content") {
                push_content(c, &mut out);
            }
            // OpenAI tool calls carried on the assistant message.
            if let Some(calls) = m.get("tool_calls").and_then(|c| c.as_array()) {
                for c in calls {
                    if let Some(f) = c.pointer("/function/name").and_then(|x| x.as_str()) {
                        out.push_str(f);
                        out.push('\n');
                    }
                    if let Some(a) = c.pointer("/function/arguments").and_then(|x| x.as_str()) {
                        out.push_str(a);
                        out.push('\n');
                    }
                }
            }
            // Anthropic-style tool_result / tool_use blocks live inside content.
            out.push('\n');
        }
    }
    // Tool definitions are part of the prompt the model sees.
    if let Some(tools) = v.get("tools") {
        push_all_strings(tools, &mut out);
    }
    out
}

/// Flatten a non-streaming response body's assistant text.
pub fn response_text(v: &serde_json::Value) -> String {
    let mut out = String::new();
    if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
        for c in choices {
            if let Some(msg) = c.get("message") {
                if let Some(t) = msg.get("content") {
                    push_content(t, &mut out);
                }
                if let Some(calls) = msg.get("tool_calls").and_then(|x| x.as_array()) {
                    for call in calls {
                        if let Some(a) = call.pointer("/function/arguments").and_then(|x| x.as_str()) {
                            out.push_str(a);
                            out.push('\n');
                        }
                    }
                }
            }
        }
    }
    if let Some(content) = v.get("content") {
        push_content(content, &mut out);
    }
    out
}

fn push_content(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        serde_json::Value::Array(items) => {
            for it in items {
                if let Some(s) = it.get("text").and_then(|x| x.as_str()) {
                    out.push_str(s);
                    out.push('\n');
                } else if let Some(s) = it.get("content").and_then(|x| x.as_str()) {
                    out.push_str(s);
                    out.push('\n');
                } else if it.get("input").is_some() {
                    push_all_strings(it.get("input").unwrap(), out);
                }
                // image_url / source blocks: not text, skipped.
            }
        }
        serde_json::Value::Object(o) => {
            if let Some(s) = o.get("text").and_then(|x| x.as_str()) {
                out.push_str(s);
                out.push('\n');
            }
        }
        _ => {}
    }
}

fn push_all_strings(v: &serde_json::Value, out: &mut String) {
    match v {
        serde_json::Value::String(s) => {
            out.push_str(s);
            out.push('\n');
        }
        serde_json::Value::Array(a) => a.iter().for_each(|e| push_all_strings(e, out)),
        serde_json::Value::Object(o) => o.values().for_each(|e| push_all_strings(e, out)),
        _ => {}
    }
}

/// Text carried by one streamed delta line (OpenAI content/reasoning/tool args,
/// Anthropic text/thinking/partial_json). Appends to `out`; returns nothing.
pub fn append_delta_text(v: &serde_json::Value, proto: crate::protocol::Protocol, out: &mut String) {
    use crate::protocol::Protocol;
    match proto {
        Protocol::OpenAi => {
            if let Some(choices) = v.get("choices").and_then(|c| c.as_array()) {
                for c in choices {
                    let Some(d) = c.get("delta") else { continue };
                    for key in ["content", "reasoning_content"] {
                        if let Some(s) = d.get(key).and_then(|x| x.as_str()) {
                            if !s.is_empty() {
                                out.push_str(s);
                            }
                        }
                    }
                    if let Some(calls) = d.get("tool_calls").and_then(|x| x.as_array()) {
                        for call in calls {
                            if let Some(a) = call.pointer("/function/arguments").and_then(|x| x.as_str()) {
                                out.push_str(a);
                            }
                        }
                    }
                }
            }
        }
        Protocol::Anthropic => {
            if let Some(d) = v.get("delta") {
                for key in ["text", "thinking", "partial_json"] {
                    if let Some(s) = d.get(key).and_then(|x| x.as_str()) {
                        if !s.is_empty() {
                            out.push_str(s);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn counts_are_real_bpe_not_char_division() {
        // 12 CJK chars: a chars/4 guess would say 3, the BPE encoder says ~12.
        let n = count("你好世界，我是一个测试");
        assert!(n >= 8, "CJK text must not be undercounted, got {n}");
        assert_eq!(count(""), 0);
    }

    #[test]
    fn payload_text_includes_messages_and_tools() {
        let v = json!({
            "model": "m",
            "messages": [
                {"role": "system", "content": "be nice"},
                {"role": "user", "content": [{"type": "text", "text": "hello"}]}
            ],
            "tools": [{"type": "function", "function": {"name": "f", "description": "does f"}}]
        });
        let t = payload_text(&v);
        assert!(t.contains("be nice"));
        assert!(t.contains("hello"));
        assert!(t.contains("does f"));
    }

    #[test]
    fn delta_text_collects_openai_and_anthropic() {
        use crate::protocol::Protocol;
        let mut s = String::new();
        append_delta_text(
            &json!({"choices":[{"delta":{"content":"he","reasoning_content":"think "}}]}),
            Protocol::OpenAi,
            &mut s,
        );
        assert_eq!(s, "hethink ");
        let mut a = String::new();
        append_delta_text(&json!({"delta":{"text":"你"}}), Protocol::Anthropic, &mut a);
        assert_eq!(a, "你");
    }
}
