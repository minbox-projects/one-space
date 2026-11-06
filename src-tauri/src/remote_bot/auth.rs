use super::types::RemoteBotConfig;

/// 鉴权验证器
pub struct AuthGuard;

impl AuthGuard {
    /// 检查指定 chat_id 是否已被授权
    pub fn is_authorized(chat_id: i64, config: &RemoteBotConfig) -> bool {
        config.allowed_chat_ids.contains(&chat_id)
    }

    /// 首次绑定逻辑：
    /// 当白名单为空时，允许首个发送 /bind 的管理员完成初始化绑定
    pub fn can_initial_bind(config: &RemoteBotConfig) -> bool {
        config.allowed_chat_ids.is_empty()
    }
}
