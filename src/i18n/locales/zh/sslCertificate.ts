// SSL certificate tool and schedule settings. Keys are grouped by feature prefix; see ../index.ts for the partition rule.
const sslCertificate = {
  "sslCertificate": "SSL 证书",
  "sslCertificateDesc": "免费申请域名证书，TLS握手健康检测、证书链分析与到期监控。",
  "sslCertificateSettingsTitle": "SSL 证书巡检与告警设置",
  "sslCertificateSettingsDesc": "配置域名 SSL 证书的每日自动巡检时间节点与临期告警阈值。",
  "sslAutoCheckEnabled": "启用每日自动巡检",
  "sslAutoCheckEnabledDesc": "开启后，系统将在设定的时间节点自动对关注列表中的域名进行 TLS 握手探测。",
  "sslCheckTimesTitle": "每日自动巡检时间节点",
  "sslCheckTimesDesc": "支持配置多个 24 小时制“时:分”时间节点（例如 09:00、18:00）。",
  "addCheckTime": "添加时间节点",
  "invalidTimeFormat": "时间格式无效，请输入 HH:mm 格式",
  "duplicateTimeNode": "该时间节点已存在",
  "sslAlertThresholdTitle": "临期告警阈值（天）",
  "sslAlertThresholdDesc": "当证书剩余天数小于等于该天数时，自动标记为临期并生成消息中心通知。",
  "sslDailyReminderNotice": "⚡ 每日提醒机制说明：",
  "sslDailyReminderNoticeDetail": "当证书剩余天数 <= 7 天且未续期时，系统每天都会在自动巡检后提醒并加入消息中心，直到证书续期恢复正常。"
};

export default sslCertificate;
