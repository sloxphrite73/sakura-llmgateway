//! Bidirectional translation between the OpenAI Chat Completions and Anthropic
//! Messages wire protocols.
//!
//! The gateway's internal request pipeline is OpenAI-shaped (that's what providers
//! have always spoken). With this module, either side can be Anthropic instead:
//!
//! - Inbound `POST /v1/messages` (Anthropic client) → translated to OpenAI → sent
//!   to an `openai` upstream; the OpenAI response/stream is translated back.
//! - An `anthropic`-protocol upstream receives Anthropic-format requests (built
//!   from either inbound protocol) and its responses/streams are translated back
//!   to whichever protocol the client spoke.
//!
//! All functions are pure `serde_json::Value` transforms so they can be unit-tested
//! without HTTP.

// ---------------------------------------------------------------------------
// Protocol tag
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    OpenAi,
    Anthropic,
}

impl Protocol {
    /// Parse a provider's `protocol` field; anything unknown/unset = OpenAI.
    pub fn parse(s: &str) -> Self {
        if s.eq_ignore_ascii_case("anthropic") {
            Protocol::Anthropic
        } else {
            Protocol::OpenAi
        }
    }
}

// ---------------------------------------------------------------------------
// Requests: Anthropic -> OpenAI
// ---------------------------------------------------------------------------

/// Translate an Anthropic `/v1/messages` request body into an OpenAI
/// `/v1/chat/completions` body. `max_tokens` (required in Anthropic) maps to
/// `max_tokens`; top-level `system` becomes a leading system message; content
/// blocks (text/image) become OpenAI content parts; `tools`/`tool_choice` are
/// mapped to the function-call format; `stop_sequences` -> `stop`.
pub fn anthropic_to_openai_request(a: &serde_json::Value) -> serde_json::Value {
    let mut o = serde_json::Map::new();

    if let Some(m) = a.get("model").and_then(|m| m.as_str()) {
        o.insert("model".into(), serde_json::json!(m));
    }
    // max_tokens is required by Anthropic; default sensibly if missing.
    let max_tokens = a.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(4096);
    o.insert("max_tokens".into(), serde_json::json!(max_tokens));

    if let Some(t) = a.get("temperature") {
        if !t.is_null() {
            o.insert("temperature".into(), t.clone());
        }
    }
    if let Some(t) = a.get("top_p") {
        if !t.is_null() {
            o.insert("top_p".into(), t.clone());
        }
    }
    if let Some(s) = a.get("stop_sequences").and_then(|s| s.as_array()) {
        o.insert("stop".into(), serde_json::json!(s));
    }
    if let Some(s) = a.get("stream") {
        o.insert("stream".into(), s.clone());
    }

    // system: string OR array of content blocks.
    let mut messages: Vec<serde_json::Value> = Vec::new();
    match a.get("system") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => {
            messages.push(serde_json::json!({ "role": "system", "content": s }));
        }
        Some(serde_json::Value::Array(blocks)) => {
            let text: Vec<&str> = blocks
                .iter()
                .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
                .collect();
            if !text.is_empty() {
                messages.push(serde_json::json!({ "role": "system", "content": text.join("\n") }));
            }
        }
        _ => {}
    }

    if let Some(msgs) = a.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            let content = msg.get("content");
            match json_message_to_openai(role, content) {
                // A user message carrying tool_result blocks expands into several
                // `role: "tool"` messages — flatten them into the conversation.
                serde_json::Value::Array(converted) => messages.extend(converted),
                one => messages.push(one),
            }
        }
    }
    o.insert("messages".into(), serde_json::json!(messages));

    // tools: Anthropic {name, description, input_schema} -> OpenAI {function: {...}}
    if let Some(tools) = a.get("tools").and_then(|t| t.as_array()) {
        let openai_tools: Vec<serde_json::Value> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.get("name").cloned().unwrap_or(serde_json::json!("")),
                        "description": t.get("description").cloned().unwrap_or(serde_json::json!("")),
                        "parameters": t.get("input_schema").cloned().unwrap_or(serde_json::json!({"type": "object"})),
                    }
                })
            })
            .collect();
        o.insert("tools".into(), serde_json::json!(openai_tools));
    }

    // tool_choice mapping.
    match a.get("tool_choice") {
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "any" => {
                o.insert("tool_choice".into(), serde_json::json!("required"));
            }
            "none" => {
                o.insert("tool_choice".into(), serde_json::json!("none"));
            }
            "auto" => {
                o.insert("tool_choice".into(), serde_json::json!("auto"));
            }
            _ => {}
        },
        Some(tc) if tc.get("type").and_then(|t| t.as_str()) == Some("tool") => {
            let name = tc.get("name").cloned().unwrap_or(serde_json::json!(""));
            o.insert(
                "tool_choice".into(),
                serde_json::json!({ "type": "function", "function": { "name": name } }),
            );
        }
        _ => {}
    }

    serde_json::Value::Object(o)
}

/// Convert one Anthropic message's content (string or block array) into an
/// OpenAI message. Tool results in user messages become `role: "tool"` messages.
fn json_message_to_openai(role: &str, content: Option<&serde_json::Value>) -> serde_json::Value {
    match content {
        // Plain string: the common case.
        Some(serde_json::Value::String(s)) => serde_json::json!({ "role": role, "content": s }),
        Some(serde_json::Value::Array(blocks)) => {
            // Split blocks: text/image blocks stay with this message; tool_result
            // blocks become separate `role: "tool"` messages; tool_use blocks in
            // assistant messages become OpenAI tool_calls.
            let mut parts: Vec<serde_json::Value> = Vec::new();
            let mut tool_calls: Vec<serde_json::Value> = Vec::new();
            let mut tool_results: Vec<(String, String)> = Vec::new(); // (tool_use_id, content)
            for b in blocks {
                let btype = b.get("type").and_then(|t| t.as_str()).unwrap_or("");
                match btype {
                    "text" => {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            parts.push(serde_json::json!({ "type": "text", "text": t }));
                        }
                    }
                    "image" => {
                        if let Some(src) = b.get("source") {
                            let media = src.get("media_type").and_then(|m| m.as_str()).unwrap_or("image/png");
                            if let Some(data) = src.get("data").and_then(|d| d.as_str()) {
                                // base64 data -> OpenAI data URL
                                parts.push(serde_json::json!({
                                    "type": "image_url",
                                    "image_url": { "url": format!("data:{media};base64,{data}") }
                                }));
                            } else if let Some(url) = src.get("url").and_then(|u| u.as_str()) {
                                parts.push(serde_json::json!({
                                    "type": "image_url",
                                    "image_url": { "url": url }
                                }));
                            }
                        }
                    }
                    "tool_use" => {
                        tool_calls.push(serde_json::json!({
                            "id": b.get("id").cloned().unwrap_or(serde_json::json!("call_0")),
                            "type": "function",
                            "function": {
                                "name": b.get("name").cloned().unwrap_or(serde_json::json!("")),
                                // OpenAI wants a JSON-encoded string of the arguments.
                                "arguments": serde_json::to_string(
                                    b.get("input").unwrap_or(&serde_json::json!({}))
                                ).unwrap_or_else(|_| "{}".into()),
                            }
                        }));
                    }
                    "tool_result" => {
                        let tid = b.get("tool_use_id").and_then(|t| t.as_str()).unwrap_or("").to_string();
                        let text = match b.get("content") {
                            Some(serde_json::Value::String(s)) => s.clone(),
                            Some(serde_json::Value::Array(arr)) => arr
                                .iter()
                                .filter_map(|c| c.get("text").and_then(|t| t.as_str()))
                                .collect::<Vec<_>>()
                                .join("\n"),
                            _ => String::new(),
                        };
                        tool_results.push((tid, text));
                    }
                    _ => {} // unknown block types are dropped
                }
            }

            // If this user message contained tool_results, emit them as tool messages.
            // (Anthropic puts tool results in `user` role; OpenAI uses `role: "tool"`.)
            if !tool_results.is_empty() {
                let mut out: Vec<serde_json::Value> = Vec::new();
                for (tid, text) in tool_results {
                    out.push(serde_json::json!({
                        "role": "tool", "tool_call_id": tid, "content": text
                    }));
                }
                return serde_json::Value::Array(out);
            }

            let mut msg = serde_json::Map::new();
            msg.insert("role".into(), serde_json::json!(role));
            if parts.len() == 1 && parts[0].get("type").and_then(|t| t.as_str()) == Some("text") {
                // single text part -> plain string (broader upstream compat)
                msg.insert("content".into(), parts[0]["text"].clone());
            } else if parts.is_empty() {
                msg.insert("content".into(), serde_json::json!(null));
            } else {
                msg.insert("content".into(), serde_json::json!(parts));
            }
            if !tool_calls.is_empty() {
                msg.insert("tool_calls".into(), serde_json::json!(tool_calls));
            }
            serde_json::Value::Object(msg)
        }
        _ => serde_json::json!({ "role": role, "content": null }),
    }
}

// ---------------------------------------------------------------------------
// Requests: OpenAI -> Anthropic
// ---------------------------------------------------------------------------

/// Translate an OpenAI `/v1/chat/completions` request body into an Anthropic
/// `/v1/messages` body. The inverse of `anthropic_to_openai_request`.
pub fn openai_to_anthropic_request(o: &serde_json::Value) -> serde_json::Value {
    let mut a = serde_json::Map::new();

    if let Some(m) = o.get("model").and_then(|m| m.as_str()) {
        a.insert("model".into(), serde_json::json!(m));
    }
    // Anthropic requires max_tokens: take the OpenAI value, else 4096.
    let max_tokens = o.get("max_tokens").and_then(|t| t.as_u64()).unwrap_or(4096);
    a.insert("max_tokens".into(), serde_json::json!(max_tokens));

    if let Some(t) = o.get("temperature") {
        if !t.is_null() {
            a.insert("temperature".into(), t.clone());
        }
    }
    if let Some(t) = o.get("top_p") {
        if !t.is_null() {
            a.insert("top_p".into(), t.clone());
        }
    }
    if let Some(s) = o.get("stop").and_then(|s| s.as_array()) {
        a.insert("stop_sequences".into(), serde_json::json!(s));
    } else if let Some(s) = o.get("stop").and_then(|s| s.as_str()) {
        a.insert("stop_sequences".into(), serde_json::json!([s]));
    }
    if let Some(s) = o.get("stream") {
        a.insert("stream".into(), s.clone());
    }

    // Split leading system messages into the top-level `system` field; convert
    // the rest of the conversation into Anthropic messages/content blocks.
    let mut system_parts: Vec<String> = Vec::new();
    let mut messages: Vec<serde_json::Value> = Vec::new();
    if let Some(msgs) = o.get("messages").and_then(|m| m.as_array()) {
        for msg in msgs {
            let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
            match role {
                "system" => {
                    collect_system_text(msg, &mut system_parts);
                }
                "tool" => {
                    // OpenAI tool result -> Anthropic user message with tool_result block.
                    let tid = msg.get("tool_call_id").and_then(|t| t.as_str()).unwrap_or("");
                    let content = msg.get("content").cloned().unwrap_or(serde_json::json!(""));
                    messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": tid,
                            "content": content,
                        }]
                    }));
                }
                _ => {
                    messages.push(openai_message_to_anthropic(msg));
                }
            }
        }
    }
    if !system_parts.is_empty() {
        a.insert("system".into(), serde_json::json!(system_parts.join("\n")));
    }
    a.insert("messages".into(), serde_json::json!(messages));

    // tools: OpenAI function format -> Anthropic tool format.
    if let Some(tools) = o.get("tools").and_then(|t| t.as_array()) {
        let ant_tools: Vec<serde_json::Value> = tools
            .iter()
            .filter_map(|t| t.get("function"))
            .map(|f| {
                serde_json::json!({
                    "name": f.get("name").cloned().unwrap_or(serde_json::json!("")),
                    "description": f.get("description").cloned().unwrap_or(serde_json::json!("")),
                    "input_schema": f.get("parameters").cloned().unwrap_or(serde_json::json!({"type": "object"})),
                })
            })
            .collect();
        if !ant_tools.is_empty() {
            a.insert("tools".into(), serde_json::json!(ant_tools));
        }
    }

    // tool_choice mapping (inverse).
    match o.get("tool_choice") {
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "required" => {
                a.insert("tool_choice".into(), serde_json::json!("any"));
            }
            "none" => {
                a.insert("tool_choice".into(), serde_json::json!("none"));
            }
            "auto" => {
                a.insert("tool_choice".into(), serde_json::json!("auto"));
            }
            _ => {}
        },
        Some(tc) if tc.get("type").and_then(|t| t.as_str()) == Some("function") => {
            let name = tc.pointer("/function/name").cloned().unwrap_or(serde_json::json!(""));
            a.insert(
                "tool_choice".into(),
                serde_json::json!({ "type": "tool", "name": name }),
            );
        }
        _ => {}
    }

    serde_json::Value::Object(a)
}

/// Extract text from an OpenAI system message (string or parts array).
fn collect_system_text(msg: &serde_json::Value, out: &mut Vec<String>) {
    match msg.get("content") {
        Some(serde_json::Value::String(s)) => out.push(s.clone()),
        Some(serde_json::Value::Array(parts)) => {
            for p in parts {
                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    out.push(t.to_string());
                }
            }
        }
        _ => {}
    }
}

/// Convert one OpenAI (assistant/user) message into Anthropic format.
fn openai_message_to_anthropic(msg: &serde_json::Value) -> serde_json::Value {
    let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user");
    let mut blocks: Vec<serde_json::Value> = Vec::new();

    match msg.get("content") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => {
            blocks.push(serde_json::json!({ "type": "text", "text": s }));
        }
        Some(serde_json::Value::Array(parts)) => {
            for p in parts {
                match p.get("type").and_then(|t| t.as_str()) {
                    Some("text") => {
                        if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                            blocks.push(serde_json::json!({ "type": "text", "text": t }));
                        }
                    }
                    Some("image_url") => {
                        let url = p.pointer("/image_url/url").and_then(|u| u.as_str()).unwrap_or("");
                        // data URLs -> base64 source; plain URLs -> url source.
                        if let Some(rest) = url.strip_prefix("data:") {
                            if let Some((meta, data)) = rest.split_once(",") {
                                let media = meta.strip_suffix(";base64").unwrap_or(meta);
                                blocks.push(serde_json::json!({
                                    "type": "image",
                                    "source": { "type": "base64", "media_type": media, "data": data }
                                }));
                                continue;
                            }
                        }
                        if !url.is_empty() {
                            blocks.push(serde_json::json!({
                                "type": "image",
                                "source": { "type": "url", "url": url }
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }
        _ => {}
    }

    // Assistant tool_calls -> tool_use blocks (Anthropic keeps these in the
    // assistant message, unlike OpenAI which separates them).
    if let Some(calls) = msg.get("tool_calls").and_then(|t| t.as_array()) {
        for c in calls {
            let args_str = c.pointer("/function/arguments").and_then(|a| a.as_str()).unwrap_or("{}");
            let input: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": c.get("id").cloned().unwrap_or(serde_json::json!("call_0")),
                "name": c.pointer("/function/name").cloned().unwrap_or(serde_json::json!("")),
                "input": input,
            }));
        }
    }

    if blocks.is_empty() {
        blocks.push(serde_json::json!({ "type": "text", "text": "" }));
    }
    serde_json::json!({ "role": role, "content": blocks })
}

// ---------------------------------------------------------------------------
// Non-streaming responses: OpenAI -> Anthropic
// ---------------------------------------------------------------------------

/// Map an OpenAI finish_reason to an Anthropic stop_reason.
fn finish_reason_to_stop_reason(fr: &str) -> &'static str {
    match fr {
        "length" => "max_tokens",
        "tool_calls" | "function_call" => "tool_use",
        "content_filter" => "refusal",
        _ => "end_turn",
    }
}

/// Build an Anthropic `/v1/messages` response from an OpenAI chat completion.
pub fn openai_to_anthropic_response(o: &serde_json::Value, model: &str) -> serde_json::Value {
    let choice = o
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|c| c.first())
        .cloned()
        .unwrap_or(serde_json::json!({}));

    let mut blocks: Vec<serde_json::Value> = Vec::new();
    let message = choice.get("message").cloned().unwrap_or(serde_json::json!({}));

    // Text content (string or parts).
    match message.get("content") {
        Some(serde_json::Value::String(s)) if !s.is_empty() => {
            blocks.push(serde_json::json!({ "type": "text", "text": s }));
        }
        Some(serde_json::Value::Array(parts)) => {
            for p in parts {
                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    blocks.push(serde_json::json!({ "type": "text", "text": t }));
                }
            }
        }
        _ => {}
    }

    // tool_calls -> tool_use blocks.
    if let Some(calls) = message.get("tool_calls").and_then(|t| t.as_array()) {
        for c in calls {
            let args_str = c.pointer("/function/arguments").and_then(|a| a.as_str()).unwrap_or("{}");
            let input: serde_json::Value = serde_json::from_str(args_str).unwrap_or(serde_json::json!({}));
            blocks.push(serde_json::json!({
                "type": "tool_use",
                "id": c.get("id").cloned().unwrap_or(serde_json::json!("toolu_gateway")),
                "name": c.pointer("/function/name").cloned().unwrap_or(serde_json::json!("")),
                "input": input,
            }));
        }
    }

    if blocks.is_empty() {
        blocks.push(serde_json::json!({ "type": "text", "text": "" }));
    }

    let finish = choice
        .get("finish_reason")
        .and_then(|f| f.as_str())
        .unwrap_or("stop");
    let stop_reason = finish_reason_to_stop_reason(finish);

    serde_json::json!({
        "id": o.get("id").cloned().unwrap_or(serde_json::json!("msg_gateway")),
        "type": "message",
        "role": "assistant",
        "model": model,
        "content": blocks,
        "stop_reason": stop_reason,
        "stop_sequence": serde_json::Value::Null,
        "usage": {
            "input_tokens": o.pointer("/usage/prompt_tokens").cloned().unwrap_or(serde_json::json!(0)),
            "output_tokens": o.pointer("/usage/completion_tokens").cloned().unwrap_or(serde_json::json!(0)),
        }
    })
}

// ---------------------------------------------------------------------------
// Non-streaming responses: Anthropic -> OpenAI
// ---------------------------------------------------------------------------

/// Map an Anthropic stop_reason to an OpenAI finish_reason.
fn stop_reason_to_finish_reason(sr: &str) -> &'static str {
    match sr {
        "max_tokens" => "length",
        "tool_use" => "tool_calls",
        "stop_sequence" => "stop",
        "refusal" => "content_filter",
        _ => "stop", // end_turn and anything else
    }
}

/// Build an OpenAI chat completion from an Anthropic `/v1/messages` response.
pub fn anthropic_to_openai_response(a: &serde_json::Value, model: &str) -> serde_json::Value {
    let mut text_parts: Vec<String> = Vec::new();
    let mut tool_calls: Vec<serde_json::Value> = Vec::new();

    if let Some(blocks) = a.get("content").and_then(|c| c.as_array()) {
        for b in blocks {
            match b.get("type").and_then(|t| t.as_str()) {
                Some("text") => {
                    if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                        text_parts.push(t.to_string());
                    }
                }
                Some("tool_use") => {
                    tool_calls.push(serde_json::json!({
                        "id": b.get("id").cloned().unwrap_or(serde_json::json!("call_0")),
                        "type": "function",
                        "function": {
                            "name": b.get("name").cloned().unwrap_or(serde_json::json!("")),
                            "arguments": serde_json::to_string(
                                b.get("input").unwrap_or(&serde_json::json!({}))
                            ).unwrap_or_else(|_| "{}".into()),
                        }
                    }));
                }
                _ => {}
            }
        }
    }

    let mut message = serde_json::Map::new();
    message.insert("role".into(), serde_json::json!("assistant"));
    message.insert("content".into(), serde_json::json!(text_parts.join("")));
    if !tool_calls.is_empty() {
        message.insert("tool_calls".into(), serde_json::json!(tool_calls));
    }

    let stop_reason = a.get("stop_reason").and_then(|s| s.as_str()).unwrap_or("end_turn");
    serde_json::json!({
        "id": a.get("id").cloned().unwrap_or(serde_json::json!("chatcmpl-gateway")),
        "object": "chat.completion",
        "created": std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        "model": model,
        "choices": [{
            "index": 0,
            "message": serde_json::Value::Object(message),
            "finish_reason": stop_reason_to_finish_reason(stop_reason),
        }],
        "usage": {
            "prompt_tokens": a.pointer("/usage/input_tokens").cloned().unwrap_or(serde_json::json!(0)),
            "completion_tokens": a.pointer("/usage/output_tokens").cloned().unwrap_or(serde_json::json!(0)),
            "total_tokens": a.pointer("/usage/input_tokens").and_then(|v| v.as_u64()).unwrap_or(0)
                + a.pointer("/usage/output_tokens").and_then(|v| v.as_u64()).unwrap_or(0),
        }
    })
}

// ---------------------------------------------------------------------------
// Streaming: OpenAI chunks -> Anthropic SSE events
// ---------------------------------------------------------------------------

/// Incremental state machine that converts a stream of OpenAI `data:` chunk
/// payloads (each already parsed from JSON; `None` = the `[DONE]` sentinel) into
/// a stream of Anthropic SSE events (already serialized as `event: ...\ndata: ...\n\n`).
#[derive(Default)]
pub struct OpenAiToAnthropicStream {
    /// message_start emitted (first chunk triggers it, carrying the model name).
    started: bool,
    /// index of the current text content_block (-1 = none open).
    text_block: i64,
    /// OpenAI tool-call index -> Anthropic content_block index.
    tool_blocks: std::collections::HashMap<u64, i64>,
    next_block: i64,
    /// Accumulated output token estimate for message_delta usage.
    out_tokens: u64,
    /// finish_reason seen on the last chunk, mapped to a stop_reason for message_delta.
    pending_stop: Option<&'static str>,
    model: String,
}

impl OpenAiToAnthropicStream {
    pub fn new(model: String) -> Self {
        Self { text_block: -1, pending_stop: None, model, ..Default::default() }
    }

    /// Feed one OpenAI chunk payload (or None for `[DONE]`); get 0..n SSE events.
    pub fn feed(&mut self, chunk: Option<&serde_json::Value>) -> Vec<String> {
        let mut events = Vec::new();

        if chunk.is_none() {
            // [DONE]: close any open block, emit message_delta + message_stop.
            self.close_text_block(&mut events);
            events.push(self.message_delta("end_turn"));
            events.push(event("message_stop", &serde_json::json!({ "type": "message_stop" })));
            return events;
        }

        let chunk = chunk.unwrap();
        if !self.started {
            self.started = true;
            events.push(event(
                "message_start",
                &serde_json::json!({
                    "type": "message_start",
                    "message": {
                        "id": chunk.get("id").cloned().unwrap_or(serde_json::json!("msg_gateway")),
                        "type": "message",
                        "role": "assistant",
                        "model": self.model,
                        "content": [],
                        "usage": { "input_tokens": chunk.pointer("/usage/prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0), "output_tokens": 0 }
                    }
                }),
            ));
        }

        let delta = chunk.pointer("/choices/0/delta").cloned().unwrap_or(serde_json::json!({}));

        // Text delta -> content_block_delta on the current text block (open lazily).
        if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                if self.text_block < 0 {
                    self.text_block = self.next_block;
                    self.next_block += 1;
                    events.push(event(
                        "content_block_start",
                        &serde_json::json!({
                            "type": "content_block_start",
                            "index": self.text_block,
                            "content_block": { "type": "text", "text": "" }
                        }),
                    ));
                }
                events.push(event(
                    "content_block_delta",
                    &serde_json::json!({
                        "type": "content_block_delta",
                        "index": self.text_block,
                        "delta": { "type": "text_delta", "text": text }
                    }),
                ));
                self.out_tokens += (text.len() as u64) / 4;
            }
        }

        // Tool-call deltas -> their own tool_use blocks (opened on first fragment).
        if let Some(calls) = delta.get("tool_calls").and_then(|t| t.as_array()) {
            for c in calls {
                let idx = c.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                let block = *self.tool_blocks.entry(idx).or_insert_with(|| {
                    let b = self.next_block;
                    self.next_block += 1;
                    events.push(event(
                        "content_block_start",
                        &serde_json::json!({
                            "type": "content_block_start",
                            "index": b,
                            "content_block": {
                                "type": "tool_use",
                                "id": c.get("id").cloned().unwrap_or(serde_json::json!("toolu_gateway")),
                                "name": c.pointer("/function/name").cloned().unwrap_or(serde_json::json!("")),
                                "input": {}
                            }
                        }),
                    ));
                    b
                });
                if let Some(args) = c.pointer("/function/arguments").and_then(|a| a.as_str()) {
                    if !args.is_empty() {
                        events.push(event(
                            "content_block_delta",
                            &serde_json::json!({
                                "type": "content_block_delta",
                                "index": block,
                                "delta": { "type": "input_json_delta", "partial_json": args }
                            }),
                        ));
                    }
                }
            }
        }

        // finish_reason: close the open text block now; the stop_reason lands in
        // message_delta at [DONE]. (OpenAI sends finish_reason on the last chunk,
        // Anthropic wants stop_reason in message_delta — we remember it.)
        if let Some(fr) = chunk.pointer("/choices/0/finish_reason").and_then(|f| f.as_str()) {
            self.close_text_block(&mut events);
            self.pending_stop = Some(finish_reason_to_stop_reason(fr));
        }

        events
    }

    fn close_text_block(&mut self, events: &mut Vec<String>) {
        if self.text_block >= 0 {
            events.push(event(
                "content_block_stop",
                &serde_json::json!({ "type": "content_block_stop", "index": self.text_block }),
            ));
            self.text_block = -1;
        }
    }

    fn message_delta(&self, fallback_stop: &str) -> String {
        let stop = self.pending_stop.as_deref().unwrap_or(fallback_stop);
        event(
            "message_delta",
            &serde_json::json!({
                "type": "message_delta",
                "delta": { "stop_reason": stop, "stop_sequence": null },
                "usage": { "output_tokens": self.out_tokens }
            }),
        )
    }
}

/// Serialize one event as Anthropic-style SSE.
fn event(name: &str, payload: &serde_json::Value) -> String {
    format!("event: {name}\ndata: {}\n\n", payload)
}

// ---------------------------------------------------------------------------
// Streaming: Anthropic events -> OpenAI chunks
// ---------------------------------------------------------------------------

/// Incremental state machine converting Anthropic SSE events into OpenAI
/// `data:` chunk payloads. `feed_raw` takes one raw SSE `data:` JSON payload
/// together with its `event:` name (as parsed by the caller).
#[derive(Default)]
pub struct AnthropicToOpenAiStream {
    started: bool,
    /// finish_reason emitted already (after stop_reason arrives, [DONE] follows).
    finished: bool,
    /// Stop reason seen in message_delta.
    stop_reason: Option<&'static str>,
}

impl AnthropicToOpenAiStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one Anthropic event; get 0..n OpenAI chunk JSON strings
    /// (each already wrapped as `data: {...}\n\n`).
    pub fn feed(&mut self, event_name: &str, payload: &serde_json::Value) -> Vec<String> {
        let mut out = Vec::new();
        match event_name {
            "message_start" => {
                self.started = true;
                // No OpenAI equivalent needed until there's content; role chunk is
                // emitted with the first delta for wider client compat.
            }
            "content_block_start" => {
                if let Some(block) = payload.get("content_block") {
                    if block.get("type").and_then(|t| t.as_str()) == Some("tool_use") {
                        out.push(self.chunk(serde_json::json!({
                            "choices": [{
                                "index": 0,
                                "delta": {
                                    "tool_calls": [{
                                        "index": payload.get("index").and_then(|i| i.as_u64()).unwrap_or(0),
                                        "id": block.get("id").cloned().unwrap_or(serde_json::json!("call_0")),
                                        "type": "function",
                                        "function": {
                                            "name": block.get("name").cloned().unwrap_or(serde_json::json!("")),
                                            "arguments": "",
                                        }
                                    }]
                                }
                            }]
                        })));
                    }
                }
            }
            "content_block_delta" => {
                let idx = payload.get("index").and_then(|i| i.as_u64()).unwrap_or(0);
                if let Some(d) = payload.get("delta") {
                    match d.get("type").and_then(|t| t.as_str()) {
                        Some("text_delta") => {
                            if let Some(text) = d.get("text").and_then(|t| t.as_str()) {
                                out.push(self.chunk(serde_json::json!({
                                    "choices": [{
                                        "index": 0,
                                        "delta": { "role": "assistant", "content": text }
                                    }]
                                })));
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(args) = d.get("partial_json").and_then(|t| t.as_str()) {
                                out.push(self.chunk(serde_json::json!({
                                    "choices": [{
                                        "index": 0,
                                        "delta": {
                                            "tool_calls": [{
                                                "index": idx,
                                                "function": { "arguments": args }
                                            }]
                                        }
                                    }]
                                })));
                            }
                        }
                        _ => {}
                    }
                }
            }
            "message_delta" => {
                if let Some(sr) = payload.pointer("/delta/stop_reason").and_then(|s| s.as_str()) {
                    self.stop_reason = Some(stop_reason_to_finish_reason(sr));
                }
            }
            "message_stop" => {
                let fr = self.stop_reason.unwrap_or("stop");
                out.push(self.chunk(serde_json::json!({
                    "choices": [{
                        "index": 0,
                        "delta": {},
                        "finish_reason": fr,
                    }]
                })));
                out.push("data: [DONE]\n\n".to_string());
                self.finished = true;
            }
            _ => {} // ping / content_block_stop / error: nothing for OpenAI clients
        }
        out
    }

    fn chunk(&self, mut body: serde_json::Value) -> String {
        let obj = body.as_object_mut().unwrap();
        obj.entry("id").or_insert_with(|| serde_json::json!("chatcmpl-gateway"));
        obj.entry("object").or_insert_with(|| serde_json::json!("chat.completion.chunk"));
        obj.entry("created").or_insert_with(|| {
            serde_json::json!(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0)
            )
        });
        obj.entry("model").or_insert_with(|| serde_json::json!("gateway"));
        format!("data: {}\n\n", serde_json::Value::Object(obj.clone()))
    }
}

// ---------------------------------------------------------------------------
// count_tokens
// ---------------------------------------------------------------------------

/// Local token estimate for a /v1/messages request when the upstream has no
/// count_tokens endpoint (OpenAI upstreams). ~4 chars per token heuristic over
/// all text content plus per-message overhead.
pub fn estimate_tokens_anthropic_request(a: &serde_json::Value) -> u64 {
    let mut chars: usize = 0;
    let mut count_text = |s: &str| chars += s.len();
    if let Some(s) = a.get("system").and_then(|s| s.as_str()) {
        count_text(s);
    } else if let Some(arr) = a.get("system").and_then(|s| s.as_array()) {
        for b in arr {
            if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                count_text(t);
            }
        }
    }
    if let Some(msgs) = a.get("messages").and_then(|m| m.as_array()) {
        for m in msgs {
            match m.get("content") {
                Some(serde_json::Value::String(s)) => count_text(s),
                Some(serde_json::Value::Array(blocks)) => {
                    for b in blocks {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            count_text(t);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let msg_count = a.get("messages").and_then(|m| m.as_array()).map(|m| m.len()).unwrap_or(0) as u64;
    (chars as u64 / 4) + msg_count * 4 + 8
}

/// Extract real (input, output) token counts from an upstream chat-completion
/// *response* by its wire protocol. Returns `None` when the upstream did not
/// report `usage` (caller falls back to `estimate_response_tokens`, spec §2.2 /
/// decision ①A). Feeds the strategy `tpm` attribute.
pub fn extract_usage_tokens(body: &serde_json::Value, proto: Protocol) -> Option<(u64, u64)> {
    let u = body.get("usage")?;
    match proto {
        Protocol::OpenAi => {
            let p = u.get("prompt_tokens").and_then(|v| v.as_u64())?;
            let c = u.get("completion_tokens").and_then(|v| v.as_u64())?;
            Some((p, c))
        }
        Protocol::Anthropic => {
            let i = u.get("input_tokens").and_then(|v| v.as_u64())?;
            let o = u.get("output_tokens").and_then(|v| v.as_u64())?;
            Some((i, o))
        }
    }
}

/// Rough token estimate for an upstream *response* body when `usage` is absent
/// (spec §2.2 fallback, decision ①A): ~4 chars per token over all string leaves.
/// A conservative lower bound (output-dominated; prompt-side unknown without the
/// request) — keeps the `tpm` window non-zero until real usage arrives.
pub fn estimate_response_tokens(body: &serde_json::Value) -> u64 {
    fn walk(v: &serde_json::Value, chars: &mut u64) {
        match v {
            serde_json::Value::String(s) => *chars += s.len() as u64,
            serde_json::Value::Array(a) => a.iter().for_each(|e| walk(e, chars)),
            serde_json::Value::Object(o) => o.values().for_each(|e| walk(e, chars)),
            _ => {}
        }
    }
    let mut chars: u64 = 0;
    walk(body, &mut chars);
    chars / 4
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn anthropic_request_system_string_becomes_system_message() {
        let a = json!({
            "model": "m", "max_tokens": 100,
            "system": "be nice",
            "messages": [{"role": "user", "content": "hi"}]
        });
        let o = anthropic_to_openai_request(&a);
        let msgs = o["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0]["role"], "system");
        assert_eq!(msgs[0]["content"], "be nice");
        assert_eq!(msgs[1]["role"], "user");
        assert_eq!(o["max_tokens"], 100);
    }

    #[test]
    fn anthropic_image_block_becomes_data_url() {
        let a = json!({
            "model": "m", "max_tokens": 10,
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "text", "text": "look"},
                    {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AAAA"}}
                ]
            }]
        });
        let o = anthropic_to_openai_request(&a);
        let parts = o["messages"][0]["content"].as_array().unwrap();
        assert_eq!(parts[0]["type"], "text");
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(parts[1]["image_url"]["url"], "data:image/png;base64,AAAA");
    }

    #[test]
    fn anthropic_tools_map_to_openai_functions() {
        let a = json!({
            "model": "m", "max_tokens": 10,
            "messages": [{"role": "user", "content": "hi"}],
            "tools": [{"name": "get_weather", "description": "d", "input_schema": {"type": "object"}}],
            "tool_choice": "any"
        });
        let o = anthropic_to_openai_request(&a);
        assert_eq!(o["tools"][0]["type"], "function");
        assert_eq!(o["tools"][0]["function"]["name"], "get_weather");
        assert_eq!(o["tool_choice"], "required");
    }

    #[test]
    fn anthropic_tool_result_becomes_tool_message() {
        let a = json!({
            "model": "m", "max_tokens": 10,
            "messages": [
                {"role": "user", "content": "hi"},
                {"role": "assistant", "content": [
                    {"type": "tool_use", "id": "t1", "name": "f", "input": {"x": 1}}
                ]},
                {"role": "user", "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": "42"}
                ]}
            ]
        });
        let o = anthropic_to_openai_request(&a);
        let msgs = o["messages"].as_array().unwrap();
        assert_eq!(msgs[1]["tool_calls"][0]["function"]["name"], "f");
        assert_eq!(msgs[1]["tool_calls"][0]["function"]["arguments"], "{\"x\":1}");
        assert_eq!(msgs[2]["role"], "tool");
        assert_eq!(msgs[2]["tool_call_id"], "t1");
        assert_eq!(msgs[2]["content"], "42");
    }

    #[test]
    fn openai_response_with_tool_calls_becomes_anthropic() {
        let o = json!({
            "id": "x", "model": "m",
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{"id": "c1", "type": "function",
                        "function": {"name": "f", "arguments": "{\"a\":2}"}}]
                },
                "finish_reason": "tool_calls"
            }],
            "usage": {"prompt_tokens": 3, "completion_tokens": 5}
        });
        let a = openai_to_anthropic_response(&o, "m");
        assert_eq!(a["stop_reason"], "tool_use");
        assert_eq!(a["content"][0]["type"], "tool_use");
        assert_eq!(a["content"][0]["input"]["a"], 2);
        assert_eq!(a["usage"]["input_tokens"], 3);
    }

    #[test]
    fn anthropic_response_becomes_openai() {
        let a = json!({
            "id": "msg_1", "type": "message", "role": "assistant",
            "content": [{"type": "text", "text": "hello"}],
            "stop_reason": "max_tokens",
            "usage": {"input_tokens": 7, "output_tokens": 9}
        });
        let o = anthropic_to_openai_response(&a, "m");
        assert_eq!(o["choices"][0]["message"]["content"], "hello");
        assert_eq!(o["choices"][0]["finish_reason"], "length");
        assert_eq!(o["usage"]["prompt_tokens"], 7);
        assert_eq!(o["usage"]["total_tokens"], 16);
    }

    #[test]
    fn openai_request_maps_to_anthropic() {
        let o = json!({
            "model": "m", "max_tokens": 55,
            "messages": [
                {"role": "system", "content": "sys"},
                {"role": "user", "content": "hi"}
            ],
            "stop": ["END"]
        });
        let a = openai_to_anthropic_request(&o);
        assert_eq!(a["system"], "sys");
        assert_eq!(a["max_tokens"], 55);
        assert_eq!(a["stop_sequences"], json!(["END"]));
        assert_eq!(a["messages"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn stream_openai_to_anthropic_shape() {
        let mut s = OpenAiToAnthropicStream::new("m".into());
        let c1 = json!({"id":"x","choices":[{"index":0,"delta":{"role":"assistant","content":"he"}}]});
        let c2 = json!({"choices":[{"index":0,"delta":{"content":"y"},"finish_reason":"stop"}]});
        let mut all = String::new();
        for e in s.feed(Some(&c1)) { all.push_str(&e); }
        for e in s.feed(Some(&c2)) { all.push_str(&e); }
        for e in s.feed(None) { all.push_str(&e); }
        assert!(all.contains("event: message_start"));
        assert!(all.contains("content_block_start"));
        assert!(all.contains("text_delta"));
        assert!(all.contains("content_block_stop"));
        assert!(all.contains("\"stop_reason\":\"end_turn\""));
        assert!(all.contains("event: message_stop"));
        // message_start appears twice: once on the `event:` line, once in the
        // data payload's "type" field.
        assert_eq!(all.matches("event: message_start").count(), 1);
    }

    #[test]
    fn stream_anthropic_to_openai_shape() {
        let mut s = AnthropicToOpenAiStream::new();
        let mut all = String::new();
        let mk = |name: &str, v: serde_json::Value| (name.to_string(), v);
        for (name, v) in [
            mk("message_start", json!({"type":"message_start","message":{"type":"message","role":"assistant"}})),
            mk("content_block_start", json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}})),
            mk("content_block_delta", json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hi"}})),
            mk("content_block_stop", json!({"type":"content_block_stop","index":0})),
            mk("message_delta", json!({"type":"message_delta","delta":{"stop_reason":"end_turn"}})),
            mk("message_stop", json!({"type":"message_stop"})),
        ] {
            for e in s.feed(&name, &v) { all.push_str(&e); }
        }
        assert!(all.contains("\"content\":\"hi\""));
        assert!(all.contains("\"finish_reason\":\"stop\""));
        assert!(all.contains("[DONE]"));
    }

    #[test]
    fn count_tokens_estimate_reasonable() {
        let a = json!({
            "model": "m", "max_tokens": 10,
            "system": "0123456789", // 10 chars
            "messages": [{"role": "user", "content": "abcdef"}] // 6 chars
        });
        let est = estimate_tokens_anthropic_request(&a);
        // 16 chars/4 = 4, + 1 msg * 4 + 8 = 16
        assert_eq!(est, 16);
    }
}
