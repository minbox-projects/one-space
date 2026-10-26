# Agent Note: File Sharing Public Access via an Ephemeral Cloudflare Tunnel

Status: implemented

[English](2026-10-26-file-sharing-public-tunnel.md) | 中文

## Problem

文件共享最初只通过私有 IPv4 的 HTTP 链接提供所选文件。用户需要把临时下载链接交给其他网络中的接收方，而不必先配置一条保存的端口隧道。Port Tunnel 已经拥有持久化的 Cloudflare 与 FRP 定义并绑定固定本地端口，而一次文件共享使用随机分配的端口、按会话生成的令牌以及由共享自身掌握的生命周期。复用保存的隧道会让两条独立的生命周期相互耦合，要求用户为每次共享创建或修改一条保存记录，并使临时共享出现在已保存配置列表中。由于任何拿到公网链接的人都能访问，该功能还需要可选的访问密码。

## Decision

文件共享保留稳定的 `file-sharing` 工具箱 id，并继续默认只提供局域网访问。启动输入新增可选的 `publicAccess` 与 `password`；快照新增 `passwordProtected`、`publicTunnelId`、`publicState`（`disabled`、`starting`、`running`、`error`）、`publicShareUrl` 与 `publicError`；`file_sharing_public_start` 与 `file_sharing_public_stop` 在运行中的共享上开关公网访问；`file-sharing-public-updated` 携带共享自有隧道的状态，并按当前隧道 id 过滤。

公网访问是由共享自有的临时 Cloudflare Quick Tunnel。运行时会以自身已绑定监听器的地址与端口为目标，使用保留的 `file-sharing-` 前缀命名隧道，并在每次尝试时生成新的唯一 id。该隧道与保存的隧道由同一进程监管器跟踪，但绝不写入 `tunnels.json`，绝不出现在已保存隧道的投影中，也不改变 Port Tunnel 的行为。公网 URL 组合为 `<tunnel-url>/s/<share-token>/`，局域网 URL 始终与其并列展示。

可选的访问密码保护整个共享，而不只是公网来源。填写密码时必须是 1–128 个字符且至少包含一个非空白字符；密码使用 SHA-256 哈希，内存中只保留哈希。共享提供中英双语的密码页，提交成功后写入 `onespace_share` 会话 cookie，带有 `HttpOnly`、`SameSite=Strict`、限定到 `/s/<share-token>/` 的路径，并在请求经 HTTPS 到达时附加 `Secure`。失败提交在响应前固定等待约 500 ms，且不锁定共享。

公网生命周期与局域网服务相互隔离。初始公网启动会在创建共享前检查 `cloudflared`，隧道失败时回滚刚创建的共享。运行中的公网启动会先检查依赖，失败时保持正在运行的局域网共享不变，记录错误状态，并允许通过重试创建新的隧道 id 与 URL。从工具或托盘停止共享、监听器意外退出以及应用关闭都会停止共享自有隧道；共享、令牌、密码与公网 URL 均不跨重启持久化。

## Alternatives considered

- 复用保存的 Port Tunnel 配置：否决，因为保存记录具有持久身份和固定本地端口，而共享使用随机端口、短暂生命周期和自有令牌；耦合二者会污染保存列表，并迫使用户为每次共享管理一条隧道定义。
- 为文件共享支持命名 Cloudflare 隧道、自定义主机名或 FRP：否决，因为这些模式需要账号或服务器配置，属于 Port Tunnel 的职责；Quick Tunnel 提供了该功能所需的零配置临时链接。
- 依据请求来源头判断是否受保护，或只保护公网 URL：否决，因为两种来源到达同一监听器，且来源头可被伪造；一个密码会话守护所有请求。
- 使用 HTTP 认证提示代替密码页与会话 cookie：否决，因为浏览器提示在通过二维码打开的移动客户端上表现不一致，也无法提供应用可控的会话生命周期或双语指引。
- 为失败登录增加硬锁定或复杂的速率限制：否决，因为共享是短生命周期且单用户的；固定延迟提高了简单猜测的成本，又不会引入共享状态或针对所有者的拒绝服务路径。
- 运行中公网启动失败时使整个共享失败：否决，因为公网暴露是可选的，隧道故障不能中断局域网传输。
- 持久化公网 URL 并在重启后恢复：否决，因为 Quick Tunnel URL 是临时的，且共享令牌、会话与密码有意只存在于当前进程。

## Consequences

- 公网共享依赖 `cloudflared`；缺失该二进制会阻止初始公网启动并显示安装指引，而仅局域网共享仍可继续工作。
- 公网 URL 是临时的，每次重试都会变化；局域网链接始终是明文 HTTP，因此界面会警告互联网暴露风险，以及在不受信任的局域网中密码或文件内容可能被观察。
- 文件共享没有新增持久化，也没有迁移；不携带新字段的既有启动调用仍会创建仅局域网的共享，Port Tunnel 的保存列表与运行时保持不变。
- 后端状态迁移测试与前端行为测试覆盖 URL 组合、状态映射、回滚、开关、重试、密码校验与门禁、事件过滤与清理；真实 Quick Tunnel 的可达性仍属于手工冒烟检查。
