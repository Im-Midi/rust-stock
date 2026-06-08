// ai.rs — AI Provider 抽象（OpenAI 兼容 Chat Completions 协议）
//
// 默认 DeepSeek。设置页改 base_url / model 即可切换任意兼容服务：
// OpenAI、Kimi(Moonshot)、通义、智谱、本地 Ollama(/v1) 等。命令层只管业务 prompt。

use serde_json::{json, Value};

pub const DEFAULT_BASE: &str = "https://api.deepseek.com";
pub const DEFAULT_MODEL: &str = "deepseek-chat";

#[derive(Debug, Clone)]
pub struct AiConfig {
    pub base_url: String,
    pub model: String,
    pub key: String,
}

impl AiConfig {
    /// 空串回退默认值；统一去掉 base_url 尾部斜杠
    pub fn new(key: String, base_url: Option<String>, model: Option<String>) -> Result<Self, String> {
        if key.trim().is_empty() {
            return Err("未配置 AI API Key（设置页）".into());
        }
        let base = base_url
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_BASE.into());
        let model = model
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| DEFAULT_MODEL.into());
        Ok(Self {
            base_url: base.trim().trim_end_matches('/').to_string(),
            model,
            key: key.trim().to_string(),
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/chat/completions", self.base_url)
    }
}

/// 把 AI 服务的 HTTP 错误翻成清晰中文（认证/限流/余额等常见情况）
fn friendly_http_error(status: reqwest::StatusCode, body: &str) -> String {
    // 尝试取 error.message
    let msg = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(|s| s.to_string()))
        .unwrap_or_else(|| body.chars().take(160).collect());
    match status.as_u16() {
        401 => format!("AI API Key 无效或已过期，请到设置页检查 Key（并确认账户余额）。原始信息：{msg}"),
        402 => format!("AI 账户余额不足，请充值后再试。原始信息：{msg}"),
        429 => format!("AI 请求过于频繁或额度用尽（限流），稍后再试。原始信息：{msg}"),
        s if s >= 500 => format!("AI 服务端错误（{status}），稍后再试。原始信息：{msg}"),
        _ => format!("AI 服务返回 {status}：{msg}"),
    }
}

/// 一次性问答（非流式），返回 content 文本
pub async fn chat_once(cfg: &AiConfig, messages: Vec<Value>, temperature: f64) -> Result<String, String> {
    // max_tokens 给足：详尽版推荐 reason（6×250~400字）会撞部分服务的默认输出上限导致 JSON 截断
    let body = json!({ "model": cfg.model, "messages": messages, "temperature": temperature, "max_tokens": 8000 });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(180)) // 详尽产业链分析较慢，给足超时
        .build()
        .map_err(|e| e.to_string())?;
    let resp = client
        .post(cfg.endpoint())
        .header("Authorization", format!("Bearer {}", cfg.key))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| if e.is_timeout() { "AI 响应超时（>180秒），请重试或换更快的模型".into() } else { format!("AI 请求失败: {e}") })?;
    let status = resp.status();
    let text = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(friendly_http_error(status, &text));
    }
    let v: Value = serde_json::from_str(&text).map_err(|e| format!("解析响应失败: {e}"))?;
    v["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.trim().to_string())
        .ok_or_else(|| "AI 响应缺少 content".into())
}

/// 从回答里抠出第一个 JSON 对象（模型可能包 markdown 代码块）
pub fn extract_json(content: &str) -> Result<Value, String> {
    let start = content.find('{').ok_or("AI 未返回 JSON")?;
    let end = content.rfind('}').ok_or("AI 未返回 JSON")?;
    serde_json::from_str(&content[start..=end]).map_err(|e| format!("解析 AI JSON 失败: {e}"))
}

/// 从回答里抠出 JSON 数组。先试标准解析；失败（常因输出被 max_tokens 截断、
/// 数组尾部不完整）时，逐个抢救已完整的顶层对象拼成数组——只要有 1 个完整对象就不报错。
pub fn extract_json_array(content: &str) -> Result<Value, String> {
    let start = content.find('[').ok_or("AI 未返回 JSON 数组")?;
    if let Some(end) = content.rfind(']') {
        if end > start {
            if let Ok(v) = serde_json::from_str::<Value>(&content[start..=end]) {
                return Ok(v);
            }
        }
    }
    // 容错：扫描提取所有完整的顶层 {...} 对象（正确处理字符串与转义）
    let objs = salvage_objects(&content[start..]);
    if objs.is_empty() {
        return Err("AI 返回的 JSON 数组无法解析（可能被截断），请重试".into());
    }
    Ok(Value::Array(objs))
}

/// 从片段里逐个提取完整的顶层 JSON 对象（括号深度计数 + 字符串状态机）
fn salvage_objects(s: &str) -> Vec<Value> {
    let mut out = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            let mut depth = 0i32;
            let mut in_str = false;
            let mut esc = false;
            let mut j = i;
            while j < bytes.len() {
                let c = bytes[j];
                if in_str {
                    if esc { esc = false; }
                    else if c == b'\\' { esc = true; }
                    else if c == b'"' { in_str = false; }
                } else {
                    match c {
                        b'"' => in_str = true,
                        b'{' => depth += 1,
                        b'}' => {
                            depth -= 1;
                            if depth == 0 {
                                if let Ok(v) = serde_json::from_str::<Value>(&s[i..=j]) {
                                    out.push(v);
                                }
                                i = j;
                                break;
                            }
                        }
                        _ => {}
                    }
                }
                j += 1;
            }
            if depth != 0 { break; } // 末尾对象不完整，停止
        }
        i += 1;
    }
    out
}

/// 流式问答：每个增量调一次 on_delta。SSE 分包可能不按行对齐，必须攒 buffer 按行切。
pub async fn chat_stream(
    cfg: &AiConfig,
    messages: Vec<Value>,
    temperature: f64,
    mut on_delta: impl FnMut(&str),
) -> Result<(), String> {
    use futures_util::StreamExt;
    let body = json!({ "model": cfg.model, "messages": messages, "stream": true, "temperature": temperature });
    let resp = reqwest::Client::new()
        .post(cfg.endpoint())
        .header("Authorization", format!("Bearer {}", cfg.key))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .map_err(|e| format!("AI 请求失败: {e}"))?;
    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return Err(friendly_http_error(status, &text));
    }
    let mut stream = resp.bytes_stream();
    let mut buf = String::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("流中断: {e}"))?;
        buf.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(pos) = buf.find('\n') {
            let line = buf[..pos].trim().to_string();
            buf.drain(..=pos);
            let Some(data) = line.strip_prefix("data:") else { continue };
            let data = data.trim();
            if data == "[DONE]" {
                return Ok(());
            }
            if let Ok(v) = serde_json::from_str::<Value>(data) {
                if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                    if !delta.is_empty() {
                        on_delta(delta);
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let c = AiConfig::new("sk-x".into(), None, None).unwrap();
        assert_eq!(c.base_url, DEFAULT_BASE);
        assert_eq!(c.model, DEFAULT_MODEL);
        assert!(AiConfig::new("  ".into(), None, None).is_err());
    }

    #[test]
    fn test_config_custom_provider() {
        let c = AiConfig::new(
            "sk-x".into(),
            Some("https://api.moonshot.cn/v1/".into()),
            Some("kimi-k2".into()),
        )
        .unwrap();
        assert_eq!(c.base_url, "https://api.moonshot.cn/v1"); // 尾部斜杠被去掉
        assert_eq!(c.endpoint(), "https://api.moonshot.cn/v1/chat/completions");
        assert_eq!(c.model, "kimi-k2");
    }

    #[test]
    fn test_extract_json_array() {
        let v = extract_json_array("好的：```json\n[{\"code\":\"sh600519\"},{\"code\":\"sz000001\"}]\n```").unwrap();
        assert_eq!(v.as_array().unwrap().len(), 2);
        assert!(extract_json_array("没有数组").is_err());
    }

    #[test]
    fn test_extract_json_array_truncated() {
        // 模拟被 max_tokens 截断：第3个对象不完整、数组无收尾 ]
        let s = "[{\"code\":\"sh600519\",\"reason\":\"含[方括号]和{花括号}的文本\"},{\"code\":\"sz000001\",\"reason\":\"完整\"},{\"code\":\"sz300";
        let v = extract_json_array(s).unwrap();
        let arr = v.as_array().unwrap();
        assert_eq!(arr.len(), 2); // 抢救出前两个完整对象，丢弃截断的第三个
        assert_eq!(arr[0]["code"], "sh600519");
        assert_eq!(arr[1]["code"], "sz000001");
    }

    #[test]
    fn test_extract_json() {
        let v = extract_json("```json\n{\"score\": -12, \"analysis\": \"理由\"}\n```").unwrap();
        assert_eq!(v["score"].as_i64(), Some(-12));
        assert!(extract_json("没有 json").is_err());
    }
}
