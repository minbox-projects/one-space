use std::time::Duration;
use reqwest::Client;

use super::types::{
    AnswerCallbackQueryPayload, EditMessageTextPayload, InlineKeyboardMarkup, SendMessagePayload,
    TelegramMessage, TelegramResponse, TelegramUpdate, TelegramUser,
};

#[derive(Clone)]
pub struct TelegramClient {
    token: String,
    base_url: String,
}

impl TelegramClient {
    pub fn new(token: impl Into<String>) -> Self {
        let token_str = token.into();
        let base_url = format!("https://api.telegram.org/bot{}", token_str);

        Self {
            token: token_str,
            base_url,
        }
    }

    pub fn token(&self) -> &str {
        &self.token
    }

    /// 获取注入了 OneSpace 统一网络代理策略的 HTTP 客户端
    fn http_client(&self, timeout: Duration) -> Result<Client, String> {
        let builder = Client::builder().timeout(timeout);
        let builder = crate::proxy::apply_current_proxy_to_builder(builder)
            .map_err(|e| format!("应用 OneSpace 统一代理策略失败: {}", e))?;
        builder.build().map_err(|e| format!("构建 HTTP 传输客户端失败: {}", e))
    }

    /// 测试当前 Token 是否有效并获取 Bot 用户信息
    pub async fn get_me(&self) -> Result<TelegramUser, String> {
        let client = self.http_client(Duration::from_secs(15))?;
        let url = format!("{}/getMe", self.base_url);
        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| {
                format!(
                    "连接 Telegram API 失败: {}。请检查网络，或在「设置 -> 服务 -> 网络代理」中配置并开启统一网络代理",
                    e
                )
            })?;

        let body: TelegramResponse<TelegramUser> = resp
            .json()
            .await
            .map_err(|e| format!("解析 getMe 响应失败: {}", e))?;

        if body.ok {
            body.result.ok_or_else(|| "getMe 返回内容为空".to_string())
        } else {
            Err(body.description.unwrap_or_else(|| "Telegram 返回未知错误".to_string()))
        }
    }

    /// 长轮询获取最新 Updates
    pub async fn get_updates(
        &self,
        offset: Option<i64>,
        timeout_secs: u64,
    ) -> Result<Vec<TelegramUpdate>, String> {
        let client = self.http_client(Duration::from_secs(timeout_secs + 15))?;
        let mut url = format!("{}/getUpdates?timeout={}", self.base_url, timeout_secs);
        if let Some(off) = offset {
            url.push_str(&format!("&offset={}", off));
        }

        let resp = client
            .get(&url)
            .send()
            .await
            .map_err(|e| format!("getUpdates 轮询失败: {}", e))?;

        let body: TelegramResponse<Vec<TelegramUpdate>> = resp
            .json()
            .await
            .map_err(|e| format!("解析 getUpdates 响应失败: {}", e))?;

        if body.ok {
            Ok(body.result.unwrap_or_default())
        } else {
            Err(body.description.unwrap_or_else(|| "getUpdates 失败".to_string()))
        }
    }

    /// 发送消息（支持 Markdown / HTML 与 Inline 键盘）
    pub async fn send_message(
        &self,
        chat_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
        parse_mode: Option<&str>,
    ) -> Result<TelegramMessage, String> {
        let client = self.http_client(Duration::from_secs(20))?;
        let url = format!("{}/sendMessage", self.base_url);
        let payload = SendMessagePayload {
            chat_id,
            text: text.to_string(),
            parse_mode: parse_mode.map(|s| s.to_string()),
            reply_markup,
        };

        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("sendMessage 失败: {}", e))?;

        let body: TelegramResponse<TelegramMessage> = resp
            .json()
            .await
            .map_err(|e| format!("解析 sendMessage 响应失败: {}", e))?;

        if body.ok {
            body.result.ok_or_else(|| "sendMessage 返回内容为空".to_string())
        } else {
            let desc = body.description.unwrap_or_else(|| "sendMessage 失败".to_string());
            // 如果遇到 Telegram Markdown 实体解析异常，自动降级为纯文本重发，确保消息 100% 成功触达
            if parse_mode.is_some() && desc.contains("can't parse entities") {
                log::warn!("Telegram Markdown 实体解析失败，自动降级为纯文本重新发送: {}", desc);
                let fallback_payload = SendMessagePayload {
                    chat_id,
                    text: text.to_string(),
                    parse_mode: None,
                    reply_markup: payload.reply_markup.clone(),
                };
                match client.post(&url).json(&fallback_payload).send().await {
                    Ok(retry_resp) => {
                        if let Ok(retry_body) = retry_resp.json::<TelegramResponse<TelegramMessage>>().await {
                            if retry_body.ok {
                                return retry_body.result.ok_or_else(|| "sendMessage 降级返回内容为空".to_string());
                            } else {
                                let retry_desc = retry_body.description.unwrap_or_default();
                                log::error!("Telegram 纯文本降级仍然失败: {}", retry_desc);
                                return Err(retry_desc);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Telegram 降级请求网络异常: {}", e);
                    }
                }
            }
            Err(desc)
        }
    }

    /// 原地编辑消息（支持进度条与状态刷新）
    pub async fn edit_message_text(
        &self,
        chat_id: i64,
        message_id: i64,
        text: &str,
        reply_markup: Option<InlineKeyboardMarkup>,
        parse_mode: Option<&str>,
    ) -> Result<TelegramMessage, String> {
        let client = self.http_client(Duration::from_secs(20))?;
        let url = format!("{}/editMessageText", self.base_url);
        let payload = EditMessageTextPayload {
            chat_id,
            message_id,
            text: text.to_string(),
            parse_mode: parse_mode.map(|s| s.to_string()),
            reply_markup,
        };

        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("editMessageText 失败: {}", e))?;

        let body: TelegramResponse<TelegramMessage> = resp
            .json()
            .await
            .map_err(|e| format!("解析 editMessageText 响应失败: {}", e))?;

        if body.ok {
            body.result.ok_or_else(|| "editMessageText 返回内容为空".to_string())
        } else {
            let desc = body.description.unwrap_or_else(|| "editMessageText 失败".to_string());
            // 如果遇到 Telegram Markdown 实体解析异常，自动降级为纯文本重发
            if parse_mode.is_some() && desc.contains("can't parse entities") {
                log::warn!("Telegram Markdown 编辑实体解析失败，自动降级为纯文本重试: {}", desc);
                let fallback_payload = EditMessageTextPayload {
                    chat_id,
                    message_id,
                    text: text.to_string(),
                    parse_mode: None,
                    reply_markup: payload.reply_markup.clone(),
                };
                match client.post(&url).json(&fallback_payload).send().await {
                    Ok(retry_resp) => {
                        if let Ok(retry_body) = retry_resp.json::<TelegramResponse<TelegramMessage>>().await {
                            if retry_body.ok {
                                return retry_body.result.ok_or_else(|| "editMessageText 降级返回内容为空".to_string());
                            } else {
                                let retry_desc = retry_body.description.unwrap_or_default();
                                log::error!("Telegram 纯文本编辑降级仍然失败: {}", retry_desc);
                                return Err(retry_desc);
                            }
                        }
                    }
                    Err(e) => {
                        log::error!("Telegram 编辑降级请求网络异常: {}", e);
                    }
                }
            }
            Err(desc)
        }
    }

    /// 响应内联键盘点击回调
    pub async fn answer_callback_query(
        &self,
        callback_query_id: &str,
        text: Option<&str>,
    ) -> Result<bool, String> {
        let client = self.http_client(Duration::from_secs(10))?;
        let url = format!("{}/answerCallbackQuery", self.base_url);
        let payload = AnswerCallbackQueryPayload {
            callback_query_id: callback_query_id.to_string(),
            text: text.map(|s| s.to_string()),
            show_alert: None,
        };

        let resp = client
            .post(&url)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("answerCallbackQuery 失败: {}", e))?;

        let body: TelegramResponse<bool> = resp
            .json()
            .await
            .map_err(|e| format!("解析 answerCallbackQuery 响应失败: {}", e))?;

        if body.ok {
            Ok(body.result.unwrap_or(true))
        } else {
            Err(body.description.unwrap_or_else(|| "answerCallbackQuery 失败".to_string()))
        }
    }
}
