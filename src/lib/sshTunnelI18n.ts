import type { TFunction } from "i18next";

const TAURI_ERROR_PREFIX = /^Error:\s*/i;

function normalizeErrorText(error: unknown): string {
  if (error instanceof Error) {
    return error.message.replace(TAURI_ERROR_PREFIX, "").trim();
  }
  return String(error ?? "")
    .replace(TAURI_ERROR_PREFIX, "")
    .trim();
}

function localizeSystemErrorDetail(t: TFunction, detail: string): string {
  return [
    {
      pattern: /Connection refused/gi,
      replacement: t("sshTunnelErrorDetailConnectionRefused", "Connection refused"),
    },
    {
      pattern: /Operation timed out/gi,
      replacement: t("sshTunnelErrorDetailTimedOut", "Operation timed out"),
    },
    {
      pattern: /\btimed out\b/gi,
      replacement: t("sshTunnelErrorDetailTimedOut", "Operation timed out"),
    },
    {
      pattern: /Connection reset by peer/gi,
      replacement: t("sshTunnelErrorDetailConnectionReset", "Connection reset by peer"),
    },
    {
      pattern: /Broken pipe/gi,
      replacement: t("sshTunnelErrorDetailBrokenPipe", "Broken pipe"),
    },
    {
      pattern: /No route to host/gi,
      replacement: t("sshTunnelErrorDetailNoRoute", "No route to host"),
    },
    {
      pattern: /Operation not permitted/gi,
      replacement: t("sshTunnelErrorDetailOperationNotPermitted", "Operation not permitted"),
    },
    {
      pattern: /Connection aborted/gi,
      replacement: t("sshTunnelErrorDetailConnectionAborted", "Connection aborted"),
    },
    {
      pattern: /Network is unreachable/gi,
      replacement: t("sshTunnelErrorDetailNetworkUnreachable", "Network is unreachable"),
    },
    {
      pattern: /Host is unreachable/gi,
      replacement: t("sshTunnelErrorDetailHostUnreachable", "Host is unreachable"),
    },
    {
      pattern: /Name or service not known/gi,
      replacement: t("sshTunnelErrorDetailNameNotKnown", "Name or service not known"),
    },
    {
      pattern: /No such host is known/gi,
      replacement: t("sshTunnelErrorDetailNameNotKnown", "No such host is known"),
    },
    {
      pattern: /nodename nor servname provided, or not known/gi,
      replacement: t(
        "sshTunnelErrorDetailNameNotKnown",
        "The host name or service name could not be resolved",
      ),
    },
    {
      pattern: /failed to lookup address information/gi,
      replacement: t(
        "sshTunnelErrorDetailAddressLookupFailed",
        "Failed to look up address information",
      ),
    },
    {
      pattern: /Permission denied/gi,
      replacement: t("sshTunnelErrorDetailPermissionDenied", "Permission denied"),
    },
    {
      pattern: /Username\/PublicKey combination invalid/gi,
      replacement: t(
        "sshTunnelErrorDetailPublicKeyRejected",
        "The username and public key combination was rejected",
      ),
    },
  ].reduce(
    (message, item) => message.replace(item.pattern, item.replacement),
    detail.trim(),
  );
}

export function localizeSshTunnelError(t: TFunction, error: unknown): string {
  const text = normalizeErrorText(error);
  if (!text) {
    return t("sshTunnelErrorUnknownShort", "SSH tunnel error");
  }

  switch (text) {
    case "Environment group name is required":
      return t("sshTunnelGroupNameRequired", "Please enter an environment group name.");
    case "The default environment group name is reserved":
      return t(
        "sshTunnelDefaultGroupNameReserved",
        '"Default Group" is reserved by the system. Please choose another name.',
      );
    case "An environment group with this name already exists":
      return t("sshTunnelGroupNameDuplicate", "An environment group with this name already exists.");
    case "The default environment group cannot be renamed":
      return t("sshTunnelDefaultGroupImmutable", "The default group cannot be renamed.");
    case "The default environment group cannot be deleted":
      return t("sshTunnelDefaultGroupDeleteForbidden", "The default group cannot be deleted.");
    case "Environment group not found":
      return t("sshTunnelGroupNotFound", "The environment group does not exist or was deleted.");
    case "Tunnel name is required":
      return t("sshTunnelErrorTunnelNameRequired", "Please enter a tunnel name.");
    case "Please choose an SSH server alias":
      return t("sshTunnelErrorSavedServerRequired", "Please choose an SSH server.");
    case "Custom SSH server details are required":
      return t("sshTunnelErrorCustomDetailsRequired", "Custom SSH details are required.");
    case "Custom SSH host and username are required":
      return t(
        "sshTunnelErrorCustomHostUserRequired",
        "Custom SSH host and username are required.",
      );
    case "Custom SSH port is invalid":
      return t("sshTunnelErrorCustomPortInvalid", "The custom SSH port is invalid.");
    case "Password authentication requires a password":
      return t("sshTunnelErrorPasswordRequired", "Password authentication requires a password.");
    case "Key authentication requires a key file":
      return t("sshTunnelErrorKeyFileRequired", "Key authentication requires a key file.");
    case "Local forwarding requires a local port":
      return t(
        "sshTunnelErrorLocalPortRequired",
        "Local forwarding requires a local port.",
      );
    case "Local forwarding requires a target host and target port":
      return t(
        "sshTunnelErrorLocalTargetRequired",
        "Local forwarding requires a target host and target port.",
      );
    case "Remote forwarding requires a remote port":
      return t(
        "sshTunnelErrorRemotePortRequired",
        "Remote forwarding requires a remote port.",
      );
    case "Remote forwarding requires a local target host and target port":
      return t(
        "sshTunnelErrorRemoteLocalTargetRequired",
        "Remote forwarding requires a service host and port on this device.",
      );
    case "Dynamic forwarding requires a local SOCKS port":
      return t(
        "sshTunnelErrorDynamicPortRequired",
        "Dynamic forwarding requires a local SOCKS port.",
      );
    case "Dynamic probe host and port must be provided together":
      return t(
        "sshTunnelErrorDynamicProbePairRequired",
        "Probe host and port must be filled in together.",
      );
    case "Could not find home directory":
      return t(
        "sshTunnelErrorHomeDirUnavailable",
        "Could not find the current user's home directory.",
      );
    case "Unrecognized SSH tunnel state payload":
      return t(
        "sshTunnelErrorStatePayloadInvalid",
        "The saved SSH tunnel state format is not recognized.",
      );
    case "The SSH server did not provide a host key":
      return t(
        "sshTunnelErrorHostKeyMissing",
        "The SSH server did not provide a host key.",
      );
    case "Failed to verify the SSH host key":
      return t(
        "sshTunnelErrorHostKeyVerifyFailed",
        "Failed to verify the SSH host key.",
      );
    case "SSH authentication failed":
      return t("sshTunnelErrorAuthFailed", "SSH authentication failed.");
    case "SSH channel closed while forwarding data":
      return t(
        "sshTunnelErrorChannelClosed",
        "The SSH channel closed while forwarding data.",
      );
    case "Local socket closed while forwarding data":
      return t(
        "sshTunnelErrorLocalSocketClosed",
        "The local socket closed while forwarding data.",
      );
    case "Forwarding thread panicked":
      return t(
        "sshTunnelErrorForwardingThreadPanicked",
        "The forwarding worker stopped unexpectedly.",
      );
    case "Invalid SOCKS5 request version":
      return t(
        "sshTunnelErrorInvalidSocksRequestVersion",
        "The SOCKS5 request version is invalid.",
      );
    case "Only SOCKS5 CONNECT is supported":
      return t(
        "sshTunnelErrorUnsupportedSocksConnect",
        "Only SOCKS5 CONNECT requests are supported.",
      );
    case "Unsupported SOCKS5 address type":
      return t(
        "sshTunnelErrorUnsupportedSocksAddressType",
        "The SOCKS5 address type is not supported.",
      );
    case "Invalid SOCKS5 greeting":
      return t("sshTunnelErrorInvalidSocksGreeting", "The SOCKS5 greeting is invalid.");
    case "SOCKS5 client does not support no-auth mode":
      return t(
        "sshTunnelErrorSocksNoAuthUnsupported",
        "The SOCKS5 client does not support no-auth mode.",
      );
    case "Timed out while establishing the SSH tunnel":
      return t(
        "sshTunnelErrorEstablishTimeout",
        "Timed out while establishing the SSH tunnel.",
      );
    case "The temporary SOCKS5 probe failed during negotiation":
      return t(
        "sshTunnelErrorTempSocksProbeNegotiationFailed",
        "The temporary SOCKS5 probe failed during negotiation.",
      );
    case "Dynamic probe thread panicked":
      return t(
        "sshTunnelErrorDynamicProbeThreadPanicked",
        "The dynamic probe worker stopped unexpectedly.",
      );
    case "Tunnel not found":
      return t("sshTunnelErrorTunnelNotFound", "The SSH tunnel does not exist.");
    case "SSH public key authentication failed with all configured identities.":
      return t(
        "sshTunnelErrorAllConfiguredKeysRejected",
        "All configured SSH public keys were rejected.",
      );
  }

  let match = text.match(
    /^Host key mismatch for (.+)\. Please inspect ~\/\.ssh\/known_hosts before retrying\.$/,
  );
  if (match) {
    return t(
      "sshTunnelErrorHostKeyMismatch",
      "Host key mismatch for {{addr}}. Please inspect ~/.ssh/known_hosts before retrying.",
      { addr: match[1] },
    );
  }

  match = text.match(
    /^SSH agent authentication failed for '(.+)'\. If this server requires a password, please create the tunnel with Custom SSH instead\.$/,
  );
  if (match) {
    return t(
      "sshTunnelErrorAgentAuthFailed",
      "SSH agent authentication failed for '{{source}}'. If this server requires a password, please create the tunnel with Custom SSH instead.",
      { source: match[1] },
    );
  }

  match = text.match(/^Could not resolve SSH server (.+)$/);
  if (match) {
    return t("sshTunnelErrorResolveSshServer", "Could not resolve SSH server {{addr}}.", {
      addr: match[1],
    });
  }

  match = text.match(/^Failed to connect to SSH server (.+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorConnectSshServer",
      "Failed to connect to SSH server {{addr}}: {{error}}",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Local port (\d+) is unavailable: (.+?)(?:\. Occupied by (.+)\.)?$/);
  if (match) {
    return t(
      "sshTunnelErrorLocalPortUnavailable",
      "Local port {{port}} is unavailable: {{error}}{{occupied}}",
      {
        port: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
        occupied: match[3]
          ? t("sshTunnelErrorLocalPortOccupiedBy", " Occupied by {{details}}.", {
              details: match[3],
            })
          : "",
      },
    );
  }

  match = text.match(/^Could not resolve target (.+)$/);
  if (match) {
    return t("sshTunnelErrorResolveTarget", "Could not resolve target {{addr}}.", {
      addr: match[1],
    });
  }

  match = text.match(/^Target service (.+) is unreachable: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorTargetServiceUnreachable",
      "The service on this device at {{addr}} is unreachable: {{error}}. Make sure it is running locally before retrying.",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Target (.+) is unreachable: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorRemoteTargetUnreachable",
      "The remote target {{addr}} is unreachable: {{error}}",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Failed to reserve remote port (.+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorReserveRemotePortFailed",
      "Failed to reserve remote port {{addr}}: {{error}}",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Failed to bind local port (\d+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorBindLocalPortFailed",
      "Failed to bind local port {{port}}: {{error}}",
      {
        port: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Failed to set non-blocking listener on (\d+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorSetListenerNonBlockingFailed",
      "Failed to configure the local listener on port {{port}}: {{error}}",
      {
        port: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Failed to clone local socket: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorCloneLocalSocketFailed",
      "Failed to clone the local socket: {{error}}",
      {
        error: localizeSystemErrorDetail(t, match[1]),
      },
    );
  }

  match = text.match(/^SOCKS probe target (.+) is unreachable: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorSocksProbeTargetUnreachable",
      "The SOCKS probe target {{addr}} is unreachable: {{error}}",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Could not resolve local target (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorResolveLocalTarget",
      "Could not resolve the service address on this device: {{addr}}.",
      {
        addr: match[1],
      },
    );
  }

  match = text.match(/^Failed to connect to local target (.+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorConnectLocalTarget",
      "Failed to connect to the service on this device at {{addr}}: {{error}}",
      {
        addr: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Public key authentication failed with (.+): (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorPublicKeyRejectedWithKey",
      "Public key authentication failed with {{key}}: {{error}}",
      {
        key: match[1],
        error: localizeSystemErrorDetail(t, match[2]),
      },
    );
  }

  match = text.match(/^Failed to start temporary SOCKS5 probe: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorTempSocksProbeStartFailed",
      "Failed to start the temporary SOCKS5 probe: {{error}}",
      {
        error: localizeSystemErrorDetail(t, match[1]),
      },
    );
  }

  match = text.match(/^Failed to connect to temporary SOCKS5 probe: (.+)$/);
  if (match) {
    return t(
      "sshTunnelErrorTempSocksProbeConnectFailed",
      "Failed to connect to the temporary SOCKS5 probe: {{error}}",
      {
        error: localizeSystemErrorDetail(t, match[1]),
      },
    );
  }

  match = text.match(/^The SOCKS5 proxy could not reach (.+) \(reply code (\d+)\)\.$/);
  if (match) {
    return t(
      "sshTunnelErrorSocksProxyReachFailed",
      "The SOCKS5 proxy could not reach {{addr}} (reply code {{code}}).",
      {
        addr: match[1],
        code: match[2],
      },
    );
  }

  const localizedDetail = localizeSystemErrorDetail(t, text);
  if (localizedDetail !== text) {
    return localizedDetail;
  }

  return t("sshTunnelErrorUnknown", "SSH tunnel error: {{error}}", { error: text });
}
