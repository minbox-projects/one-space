pub(super) fn read_protobuf_varint(data: &[u8], pos: &mut usize) -> Option<u64> {
    let mut val = 0u64;
    let mut shift = 0;
    while *pos < data.len() {
        let b = data[*pos];
        *pos += 1;
        val |= ((b & 0x7F) as u64) << shift;
        if (b & 0x80) == 0 {
            return Some(val);
        }
        shift += 7;
        if shift >= 64 {
            return None;
        }
    }
    None
}

pub(super) fn skip_protobuf_field(data: &[u8], pos: &mut usize, wire_type: u8) -> bool {
    match wire_type {
        0 => read_protobuf_varint(data, pos).is_some(),
        1 => {
            if *pos + 8 <= data.len() {
                *pos += 8;
                true
            } else {
                false
            }
        }
        2 => {
            if let Some(len) = read_protobuf_varint(data, pos) {
                let len = len as usize;
                if *pos + len <= data.len() {
                    *pos += len;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        }
        5 => {
            if *pos + 4 <= data.len() {
                *pos += 4;
                true
            } else {
                false
            }
        }
        _ => false,
    }
}

pub(super) struct AntigravityStepTokens {
    pub(super) timestamp_ms: i64,
    pub(super) input_tokens: u64,
    pub(super) output_tokens: u64,
    pub(super) cache_tokens: u64,
}

pub(super) fn parse_antigravity_step_tokens(data: &[u8]) -> Option<AntigravityStepTokens> {
    let mut pos = 0;
    let mut timestamp_ms = 0i64;
    let mut input_tokens = 0u64;
    let mut output_tokens = 0u64;
    let mut cache_tokens = 0u64;
    let mut has_tokens = false;

    while pos < data.len() {
        let tag = read_protobuf_varint(data, &mut pos)?;
        let fnum = tag >> 3;
        let wtype = (tag & 0x7) as u8;

        if wtype == 2 {
            let len = read_protobuf_varint(data, &mut pos)? as usize;
            if pos + len > data.len() {
                return None;
            }
            let sub = &data[pos..pos + len];
            pos += len;

            if fnum == 1 {
                // Timestamp { int64 seconds = 1; int32 nanos = 2; }
                let mut spos = 0;
                let mut sec = 0i64;
                let mut nano = 0i64;
                while spos < sub.len() {
                    if let Some(stag) = read_protobuf_varint(sub, &mut spos) {
                        let sfnum = stag >> 3;
                        let swtype = (stag & 0x7) as u8;
                        if swtype == 0 {
                            if let Some(sval) = read_protobuf_varint(sub, &mut spos) {
                                if sfnum == 1 {
                                    sec = sval as i64;
                                } else if sfnum == 2 {
                                    nano = sval as i64;
                                }
                            }
                        } else if !skip_protobuf_field(sub, &mut spos, swtype) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                timestamp_ms = sec.saturating_mul(1000).saturating_add(nano / 1_000_000);
            } else if fnum == 9 {
                // Tokens info
                let mut spos = 0;
                while spos < sub.len() {
                    if let Some(stag) = read_protobuf_varint(sub, &mut spos) {
                        let sfnum = stag >> 3;
                        let swtype = (stag & 0x7) as u8;
                        if swtype == 0 {
                            if let Some(sval) = read_protobuf_varint(sub, &mut spos) {
                                if sfnum == 2 {
                                    input_tokens = sval;
                                    has_tokens = true;
                                } else if sfnum == 3 {
                                    output_tokens = sval;
                                    has_tokens = true;
                                } else if sfnum == 5 {
                                    cache_tokens = sval;
                                    has_tokens = true;
                                }
                            }
                        } else if !skip_protobuf_field(sub, &mut spos, swtype) {
                            break;
                        }
                    } else {
                        break;
                    }
                }
            }
        } else if !skip_protobuf_field(data, &mut pos, wtype) {
            return None;
        }
    }

    if has_tokens && (input_tokens > 0 || output_tokens > 0 || cache_tokens > 0) {
        Some(AntigravityStepTokens {
            timestamp_ms,
            input_tokens,
            output_tokens,
            cache_tokens,
        })
    } else {
        None
    }
}

pub(super) fn parse_antigravity_gen_metadata(data: &[u8]) -> Option<(String, Option<i64>)> {
    let mut pos = 0;
    let mut model = None::<String>;
    let mut last_step_index = None::<i64>;

    while pos < data.len() {
        let tag = read_protobuf_varint(data, &mut pos)?;
        let fnum = tag >> 3;
        let wtype = (tag & 0x7) as u8;

        if fnum == 1 && wtype == 2 {
            let len = read_protobuf_varint(data, &mut pos)? as usize;
            if pos + len > data.len() {
                return None;
            }
            let sub = &data[pos..pos + len];
            pos += len;
            let mut spos = 0;
            while spos < sub.len() {
                if let Some(stag) = read_protobuf_varint(sub, &mut spos) {
                    let sfnum = stag >> 3;
                    let swtype = (stag & 0x7) as u8;
                    if sfnum == 19 && swtype == 2 {
                        if let Some(slen) = read_protobuf_varint(sub, &mut spos) {
                            let slen = slen as usize;
                            if spos + slen <= sub.len() {
                                if let Ok(s) = std::str::from_utf8(&sub[spos..spos + slen]) {
                                    model = Some(s.to_string());
                                }
                                spos += slen;
                            }
                        }
                    } else if sfnum == 20 && swtype == 2 {
                        if let Some(slen) = read_protobuf_varint(sub, &mut spos) {
                            let slen = slen as usize;
                            if spos + slen <= sub.len() {
                                let kv_sub = &sub[spos..spos + slen];
                                spos += slen;
                                let mut kpos = 0;
                                let mut k = None::<String>;
                                let mut v = None::<String>;
                                while kpos < kv_sub.len() {
                                    if let Some(ktag) = read_protobuf_varint(kv_sub, &mut kpos) {
                                        let kfnum = ktag >> 3;
                                        let kwtype = (ktag & 0x7) as u8;
                                        if kwtype == 2 {
                                            if let Some(klen) = read_protobuf_varint(kv_sub, &mut kpos) {
                                                let klen = klen as usize;
                                                if kpos + klen <= kv_sub.len() {
                                                    if let Ok(s) = std::str::from_utf8(&kv_sub[kpos..kpos + klen]) {
                                                        if kfnum == 1 {
                                                            k = Some(s.to_string());
                                                        } else if kfnum == 2 {
                                                            v = Some(s.to_string());
                                                        }
                                                    }
                                                    kpos += klen;
                                                }
                                            }
                                        } else if !skip_protobuf_field(kv_sub, &mut kpos, kwtype) {
                                            break;
                                        }
                                    } else {
                                        break;
                                    }
                                }
                                if k.as_deref() == Some("last_step_index") {
                                    if let Some(v_str) = v {
                                        last_step_index = v_str.parse::<i64>().ok();
                                    }
                                }
                            }
                        }
                    } else if !skip_protobuf_field(sub, &mut spos, swtype) {
                        break;
                    }
                } else {
                    break;
                }
            }
        } else if !skip_protobuf_field(data, &mut pos, wtype) {
            return None;
        }
    }

    model.map(|m| (m, last_step_index))
}
