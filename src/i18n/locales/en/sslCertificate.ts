// SSL certificate tool and schedule settings. Keys are grouped by feature prefix; see ../index.ts for the partition rule.
const sslCertificate = {
  "sslCertificate": "SSL Certificates",
  "sslCertificateDesc": "Free SSL certificate issuance, TLS health check, and expiration alerts.",
  "sslCertificateSettingsTitle": "SSL Certificate Inspection & Alert Settings",
  "sslCertificateSettingsDesc": "Configure daily automatic inspection time slots and expiration alert thresholds for domain SSL certificates.",
  "sslAutoCheckEnabled": "Enable Daily Auto Inspection",
  "sslAutoCheckEnabledDesc": "When enabled, the system will automatically inspect TLS health for all domains in your watchlist at configured time slots.",
  "sslCheckTimesTitle": "Daily Inspection Time Slots",
  "sslCheckTimesDesc": "Support multiple 24-hour 'HH:mm' time slots (e.g., 09:00, 18:00).",
  "addCheckTime": "Add Time Slot",
  "invalidTimeFormat": "Invalid time format, please enter in HH:mm format",
  "duplicateTimeNode": "This time slot already exists",
  "sslAlertThresholdTitle": "Expiration Alert Threshold (Days)",
  "sslAlertThresholdDesc": "When a certificate has fewer or equal remaining days than this threshold, it will be marked as expiring soon and a notification will be sent to the Message Center.",
  "sslDailyReminderNotice": "⚡ Daily Reminder Mechanism:",
  "sslDailyReminderNoticeDetail": "When remaining days are <= 7 days and the certificate has not been renewed, the system sends daily reminders to the Message Center after inspection until renewed."
};

export default sslCertificate;
