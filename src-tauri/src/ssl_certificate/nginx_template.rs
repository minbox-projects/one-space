pub fn generate_nginx_config(
    primary_domain: &str,
    san_domains: &[String],
    ssl_dir_hint: Option<&str>,
) -> String {
    let mut all_domains = vec![primary_domain.to_string()];
    for san in san_domains {
        if !all_domains.contains(san) {
            all_domains.push(san.clone());
        }
    }
    let server_names = all_domains.join(" ");
    let base_dir = ssl_dir_hint.unwrap_or("/etc/nginx/ssl");
    let domain_clean = primary_domain.replace('*', "wildcard");

    format!(
r#"# ==============================================================================
# OneSpace 自动生成的 Nginx SSL 配置样例
# 域名: {server_names}
# 生成时间: 详见证书签发记录
# 部署说明:
# 1. 将 fullchain.pem 与 privkey.pem 复制到目标服务器指定目录（如 {base_dir}/{domain_clean}/）
# 2. 检查配置正确性: nginx -t
# 3. 平滑重新加载配置: nginx -s reload
# ==============================================================================

# 1. HTTP 80 端口强制重定向到 HTTPS
server {{
    listen 80;
    listen [::]:80;
    server_name {server_names};

    # 重定向所有 HTTP 请求至 HTTPS
    return 301 https://$host$request_uri;
}}

# 2. HTTPS 443 核心安全服务配置
server {{
    listen 443 ssl http2;
    listen [::]:443 ssl http2;
    server_name {server_names};

    # SSL 证书与私钥路径 (请根据服务器实际存放路径调整)
    ssl_certificate {base_dir}/{domain_clean}/fullchain.pem;
    ssl_certificate_key {base_dir}/{domain_clean}/privkey.pem;

    # 安全协议与加密套件推荐 (Mozilla Modern / Intermediate 安全基准)
    ssl_protocols TLSv1.2 TLSv1.3;
    ssl_ciphers ECDHE-ECDSA-AES128-GCM-SHA256:ECDHE-RSA-AES128-GCM-SHA256:ECDHE-ECDSA-AES256-GCM-SHA384:ECDHE-RSA-AES256-GCM-SHA384:DHE-RSA-AES128-GCM-SHA256:DHE-RSA-AES256-GCM-SHA384;
    ssl_prefer_server_ciphers off;

    # SSL 会话缓存优化
    ssl_session_cache shared:SSL:10m;
    ssl_session_timeout 1d;
    ssl_session_tickets off;

    # HSTS 安全加固 (可选，开启后强制浏览器使用 HTTPS，确认无误后再启用)
    # add_header Strict-Transport-Security "max-age=63072000; includeSubDomains; preload" always;

    # 安全头部
    add_header X-Frame-Options SAMEORIGIN;
    add_header X-Content-Type-Options nosniff;
    add_header X-XSS-Protection "1; mode=block";

    # 业务反向代理示例 (按需取消注释或修改)
    location / {{
        # proxy_pass http://127.0.0.1:3000;
        # proxy_set_header Host $host;
        # proxy_set_header X-Real-IP $remote_addr;
        # proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        # proxy_set_header X-Forwarded-Proto $scheme;

        root /var/www/html;
        index index.html index.htm;
        try_files $uri $uri/ /index.html;
    }}
}}
"#,
        server_names = server_names,
        base_dir = base_dir,
        domain_clean = domain_clean
    )
}
