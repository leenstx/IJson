use serde_json::Value;
use std::collections::HashSet;
use std::io::Read;

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum SortMode {
    None,
    Asc,
    Desc,
}

pub(crate) struct ParseError {
    pub(crate) message: String,
    pub(crate) line: usize,
    pub(crate) column: usize,
}

pub(crate) struct EditModalState {
    pub(crate) path: String,
    pub(crate) key_label: String,
    pub(crate) input_text: String,
    pub(crate) target_type: ValueTypeTag,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Copy, PartialEq)]
pub(crate) enum ValueTypeTag {
    String,
    Number,
    Bool,
    Null,
    RawJson,
}

pub(crate) enum TreeAction {
    Select(String),
    CopyText(String),
    StartEdit(String, String, Value),
    DeleteNode(String),
    ExpandSubtree(String),
    CollapseSubtree(String, Value),
    DownloadImage(String),
}

pub(crate) fn search_recursive(v: &Value, path: &str, query: &str, matches: &mut Vec<String>) {
    match v {
        Value::Object(map) => {
            for (k, val) in map {
                let child = format!("{}.{}", path, k);
                if k.to_lowercase().contains(query) {
                    matches.push(child.clone());
                }
                search_recursive(val, &child, query, matches);
            }
        }
        Value::Array(arr) => {
            for (i, val) in arr.iter().enumerate() {
                let child = format!("{}[{}]", path, i);
                search_recursive(val, &child, query, matches);
            }
        }
        Value::String(s) => {
            if s.to_lowercase().contains(query) {
                matches.push(path.to_string());
            }
        }
        Value::Number(n) => {
            if n.to_string().contains(query) {
                matches.push(path.to_string());
            }
        }
        Value::Bool(b) => {
            if b.to_string().contains(query) {
                matches.push(path.to_string());
            }
        }
        Value::Null => {
            if "null".contains(query) {
                matches.push(path.to_string());
            }
        }
    }
}

pub(crate) fn collect_all_expandable_paths(v: &Value, path: &str, set: &mut HashSet<String>) {
    match v {
        Value::Object(map) => {
            set.insert(path.to_string());
            for (k, v) in map {
                let child = format!("{}.{}", path, k);
                collect_all_expandable_paths(v, &child, set);
            }
        }
        Value::Array(arr) => {
            set.insert(path.to_string());
            for (i, v) in arr.iter().enumerate() {
                let child = format!("{}[{}]", path, i);
                collect_all_expandable_paths(v, &child, set);
            }
        }
        _ => {}
    }
}

pub(crate) fn collapse_beyond_depth(
    v: &Value,
    path: &str,
    current_depth: usize,
    target_depth: usize,
    set: &mut HashSet<String>,
) {
    match v {
        Value::Object(map) => {
            if current_depth >= target_depth {
                set.insert(path.to_string());
            }
            for (k, v) in map {
                let child = format!("{}.{}", path, k);
                collapse_beyond_depth(v, &child, current_depth + 1, target_depth, set);
            }
        }
        Value::Array(arr) => {
            if current_depth >= target_depth {
                set.insert(path.to_string());
            }
            for (i, v) in arr.iter().enumerate() {
                let child = format!("{}[{}]", path, i);
                collapse_beyond_depth(v, &child, current_depth + 1, target_depth, set);
            }
        }
        _ => {}
    }
}

pub(crate) fn sort_json_value(v: &Value, mode: SortMode) -> Value {
    match v {
        Value::Object(map) => {
            let mut pairs: Vec<(String, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), sort_json_value(v, mode)))
                .collect();
            match mode {
                SortMode::Asc => pairs.sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase())),
                SortMode::Desc => pairs.sort_by(|a, b| b.0.to_lowercase().cmp(&a.0.to_lowercase())),
                SortMode::None => {}
            }
            Value::Object(pairs.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.iter().map(|v| sort_json_value(v, mode)).collect()),
        other => other.clone(),
    }
}

/// Check if every array in the JSON has at most one element.
pub(crate) fn is_minimal_json_value(v: &Value) -> bool {
    match v {
        Value::Object(map) => map.values().all(is_minimal_json_value),
        Value::Array(arr) => arr.len() <= 1 && arr.iter().all(is_minimal_json_value),
        _ => true,
    }
}

/// Keep field order and only the first array element, preserving leaf values.
pub(crate) fn to_minimal_json_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), to_minimal_json_value(value)))
                .collect(),
        ),
        Value::Array(arr) => {
            Value::Array(arr.first().map(to_minimal_json_value).into_iter().collect())
        }
        other => other.clone(),
    }
}

/// Keep field order and only the first array element as a structural template.
pub(crate) fn simplify_json_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), simplify_json_value(value)))
                .collect(),
        ),
        Value::Array(arr) => {
            Value::Array(arr.first().map(simplify_json_value).into_iter().collect())
        }
        Value::Number(_) => Value::from(0),
        Value::Bool(_) => Value::Bool(false),
        Value::String(_) => Value::String(String::new()),
        Value::Null => Value::Null,
    }
}

pub(crate) enum PathSegment {
    Key(String),
    Index(usize),
}

pub(crate) fn parse_json_path(path: &str) -> Vec<PathSegment> {
    let mut segments = Vec::new();
    let mut chars = path.chars().peekable();

    if chars.peek() == Some(&'$') {
        chars.next();
    }

    let mut cur_str = String::new();
    while let Some(c) = chars.next() {
        if c == '.' {
            if !cur_str.is_empty() {
                segments.push(PathSegment::Key(cur_str.clone()));
                cur_str.clear();
            }
        } else if c == '[' {
            if !cur_str.is_empty() {
                segments.push(PathSegment::Key(cur_str.clone()));
                cur_str.clear();
            }
            let mut idx_str = String::new();
            while let Some(&ic) = chars.peek() {
                if ic == ']' {
                    chars.next();
                    break;
                }
                idx_str.push(ic);
                chars.next();
            }
            if let Ok(idx) = idx_str.parse::<usize>() {
                segments.push(PathSegment::Index(idx));
            }
        } else {
            cur_str.push(c);
        }
    }
    if !cur_str.is_empty() {
        segments.push(PathSegment::Key(cur_str));
    }
    segments
}

pub(crate) fn update_value_at_path(root: &mut Value, path: &str, new_val: Value) -> bool {
    let segments = parse_json_path(path);
    if segments.is_empty() {
        *root = new_val;
        return true;
    }

    let mut current = root;
    for (i, seg) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        match seg {
            PathSegment::Key(k) => {
                if let Value::Object(map) = current {
                    if is_last {
                        map.insert(k.clone(), new_val);
                        return true;
                    } else if let Some(next) = map.get_mut(k) {
                        current = next;
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            PathSegment::Index(idx) => {
                if let Value::Array(arr) = current {
                    if *idx < arr.len() {
                        if is_last {
                            arr[*idx] = new_val;
                            return true;
                        } else {
                            current = &mut arr[*idx];
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }
    false
}

pub(crate) fn delete_value_at_path(root: &mut Value, path: &str) -> bool {
    let segments = parse_json_path(path);
    if segments.is_empty() {
        return false;
    }

    let mut current = root;
    for (i, seg) in segments.iter().enumerate() {
        let is_last = i == segments.len() - 1;
        match seg {
            PathSegment::Key(k) => {
                if let Value::Object(map) = current {
                    if is_last {
                        return map.remove(k).is_some();
                    } else if let Some(next) = map.get_mut(k) {
                        current = next;
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            PathSegment::Index(idx) => {
                if let Value::Array(arr) = current {
                    if *idx < arr.len() {
                        if is_last {
                            arr.remove(*idx);
                            return true;
                        } else {
                            current = &mut arr[*idx];
                        }
                    } else {
                        return false;
                    }
                } else {
                    return false;
                }
            }
        }
    }
    false
}

fn unescape_quotes_and_slashes(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    '"' | '\\' | '/' => {
                        out.push(next);
                        chars.next();
                    }
                    _ => {
                        out.push('\\');
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn unescape_full(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                match next {
                    '"' | '\\' | '/' => {
                        out.push(next);
                        chars.next();
                    }
                    'n' => {
                        out.push('\n');
                        chars.next();
                    }
                    'r' => {
                        out.push('\r');
                        chars.next();
                    }
                    't' => {
                        out.push('\t');
                        chars.next();
                    }
                    _ => {
                        out.push('\\');
                    }
                }
            } else {
                out.push('\\');
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn strip_outer_quotes(s: &str) -> &str {
    let trimmed = s.trim();
    if trimmed.len() >= 4 && trimmed.starts_with(r#"\""#) && trimmed.ends_with(r#"\""#) {
        return &trimmed[2..trimmed.len() - 2];
    }
    if trimmed.len() >= 2
        && ((trimmed.starts_with('"') && trimmed.ends_with('"'))
            || (trimmed.starts_with('\'') && trimmed.ends_with('\'')))
    {
        return &trimmed[1..trimmed.len() - 1];
    }
    trimmed
}

/// Try unescaping text that may be an escaped JSON string (e.g. `{\"a\":1}` or `"{\"a\":1}"`).
/// Returns the parsed `Value` and the unescaped JSON text if successful.
pub(crate) fn try_unescape_json(text: &str) -> Option<(Value, String)> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }

    let mut candidates = Vec::new();

    // 1. Direct unescape of quotes and slashes (handles `{\"a\":1}`)
    let unescaped_direct = unescape_quotes_and_slashes(trimmed);
    if unescaped_direct != trimmed {
        candidates.push(unescaped_direct.clone());
        let double_unescaped = unescape_quotes_and_slashes(&unescaped_direct);
        if double_unescaped != unescaped_direct {
            candidates.push(double_unescaped);
        }
    }

    // 2. If wrapped in outer quotes, strip them first
    let stripped = strip_outer_quotes(trimmed);
    if stripped != trimmed {
        candidates.push(stripped.to_string());
        let unescaped_stripped = unescape_quotes_and_slashes(stripped);
        if unescaped_stripped != stripped {
            candidates.push(unescaped_stripped.clone());
            let double_stripped = unescape_quotes_and_slashes(&unescaped_stripped);
            if double_stripped != unescaped_stripped {
                candidates.push(double_stripped);
            }
        }
    }

    // 3. Full unescape including \n, \r, \t
    let full_unescaped = unescape_full(stripped);
    if full_unescaped != stripped && !candidates.contains(&full_unescaped) {
        candidates.push(full_unescaped);
    }

    // 4. Try parsing with serde_json's string parser if wrapped as JSON string
    if trimmed.len() >= 2 && trimmed.starts_with('"') && trimmed.ends_with('"') {
        if let Ok(inner) = serde_json::from_str::<String>(trimmed) {
            if !candidates.contains(&inner) {
                candidates.push(inner.clone());
            }
            let unescaped_inner = unescape_quotes_and_slashes(&inner);
            if unescaped_inner != inner && !candidates.contains(&unescaped_inner) {
                candidates.push(unescaped_inner);
            }
        }
    }

    for candidate in candidates {
        if let Ok(v) = serde_json::from_str::<Value>(&candidate) {
            if let Value::String(ref inner_str) = v {
                let inner_trimmed = inner_str.trim();
                if (inner_trimmed.starts_with('{') && inner_trimmed.ends_with('}'))
                    || (inner_trimmed.starts_with('[') && inner_trimmed.ends_with(']'))
                {
                    if let Ok(inner_v) = serde_json::from_str::<Value>(inner_trimmed) {
                        return Some((inner_v, inner_trimmed.to_string()));
                    }
                }
            }
            return Some((v, candidate));
        }
    }

    None
}

struct ParsedHttpRequest {
    url: String,
    method: String,
    headers: Vec<(String, String)>,
    body: Option<String>,
}

fn parse_curl_command(args: &[String]) -> Option<ParsedHttpRequest> {
    let mut url: Option<String> = None;
    let mut method: Option<String> = None;
    let mut headers = Vec::new();
    let mut body_parts = Vec::new();

    let mut i = 0;
    if i < args.len() && (args[i] == "curl" || args[i] == "curl.exe") {
        i += 1;
    }

    while i < args.len() {
        let arg = &args[i];
        if arg == "-X" || arg == "--request" {
            if i + 1 < args.len() {
                method = Some(args[i + 1].to_ascii_uppercase());
                i += 2;
                continue;
            }
        } else if arg.starts_with("-X") && arg.len() > 2 {
            method = Some(arg[2..].to_ascii_uppercase());
            i += 1;
            continue;
        } else if arg == "-H" || arg == "--header" {
            if i + 1 < args.len() {
                let header_str = &args[i + 1];
                if let Some(colon) = header_str.find(':') {
                    headers.push((
                        header_str[..colon].trim().to_string(),
                        header_str[colon + 1..].trim().to_string(),
                    ));
                }
                i += 2;
                continue;
            }
        } else if arg.starts_with("--header=") {
            let header_str = &arg["--header=".len()..];
            if let Some(colon) = header_str.find(':') {
                headers.push((
                    header_str[..colon].trim().to_string(),
                    header_str[colon + 1..].trim().to_string(),
                ));
            }
            i += 1;
            continue;
        } else if arg.starts_with("-H") && arg.len() > 2 {
            let header_str = &arg[2..];
            if let Some(colon) = header_str.find(':') {
                headers.push((
                    header_str[..colon].trim().to_string(),
                    header_str[colon + 1..].trim().to_string(),
                ));
            }
            i += 1;
            continue;
        } else if arg == "-d"
            || arg == "--data"
            || arg == "--data-raw"
            || arg == "--data-binary"
            || arg == "--data-ascii"
            || arg == "--data-urlencode"
        {
            if i + 1 < args.len() {
                body_parts.push(args[i + 1].clone());
                i += 2;
                continue;
            }
        } else if arg.starts_with("--data=") || arg.starts_with("--data-raw=") {
            let prefix = if arg.starts_with("--data=") {
                "--data="
            } else {
                "--data-raw="
            };
            body_parts.push(arg[prefix.len()..].to_string());
            i += 1;
            continue;
        } else if arg.starts_with("-d") && arg.len() > 2 {
            body_parts.push(arg[2..].to_string());
            i += 1;
            continue;
        } else if arg == "-A" || arg == "--user-agent" {
            if i + 1 < args.len() {
                headers.push(("User-Agent".to_string(), args[i + 1].clone()));
                i += 2;
                continue;
            }
        } else if arg == "-u" || arg == "--user" {
            if i + 1 < args.len() {
                use base64::Engine as _;
                let encoded = base64::prelude::BASE64_STANDARD.encode(&args[i + 1]);
                headers.push(("Authorization".to_string(), format!("Basic {}", encoded)));
                i += 2;
                continue;
            }
        } else if arg == "-b" || arg == "--cookie" {
            if i + 1 < args.len() {
                headers.push(("Cookie".to_string(), args[i + 1].clone()));
                i += 2;
                continue;
            }
        } else if arg == "--url" {
            if i + 1 < args.len() {
                url = Some(args[i + 1].clone());
                i += 2;
                continue;
            }
        } else if arg.starts_with("--url=") {
            url = Some(arg["--url=".len()..].to_string());
            i += 1;
            continue;
        } else if matches!(
            arg.as_str(),
            "-o" | "--output"
                | "-m"
                | "--max-time"
                | "--connect-timeout"
                | "--retry"
                | "-e"
                | "--referer"
                | "--cacert"
                | "--cert"
                | "--key"
                | "-x"
                | "--proxy"
        ) {
            i += 2;
            continue;
        } else if arg.starts_with('-') {
            i += 1;
            continue;
        } else {
            // Positional argument: URL
            if url.is_none() {
                url = Some(arg.clone());
            }
            i += 1;
            continue;
        }
        i += 1;
    }

    let mut url = url?;
    if !url.starts_with("http://") && !url.starts_with("https://") {
        url = format!("http://{}", url);
    }

    let final_method = match method {
        Some(m) => m,
        None => {
            if !body_parts.is_empty() {
                "POST".to_string()
            } else {
                "GET".to_string()
            }
        }
    };

    let body = if !body_parts.is_empty() {
        Some(body_parts.join("&"))
    } else {
        None
    };

    Some(ParsedHttpRequest {
        url,
        method: final_method,
        headers,
        body,
    })
}

fn parse_http_url(text: &str) -> Option<ParsedHttpRequest> {
    let trimmed = text.trim();
    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains('\n')
        && !trimmed.contains('\r')
        && !trimmed.contains(' ')
    {
        return Some(ParsedHttpRequest {
            url: trimmed.to_string(),
            method: "GET".to_string(),
            headers: Vec::new(),
            body: None,
        });
    }

    let mut parts = trimmed.split_whitespace();
    if let (Some(m), Some(u)) = (parts.next(), parts.next()) {
        if parts.next().is_none() {
            let m_upper = m.to_ascii_uppercase();
            if matches!(m_upper.as_str(), "GET" | "POST" | "PUT" | "DELETE" | "PATCH" | "HEAD")
                && (u.starts_with("http://") || u.starts_with("https://"))
            {
                return Some(ParsedHttpRequest {
                    url: u.to_string(),
                    method: m_upper,
                    headers: Vec::new(),
                    body: None,
                });
            }
        }
    }

    None
}

/// Check if a Content-Type string indicates JSON format.
pub(crate) fn is_json_content_type(ct: Option<&str>) -> bool {
    let Some(ct) = ct else {
        return false;
    };
    let mime = ct.split(';').next().unwrap_or("").trim().to_ascii_lowercase();
    mime == "application/json"
        || mime == "text/json"
        || mime == "text/x-json"
        || mime.ends_with("+json")
}

/// Check if text is a candidate for curl or HTTP request.
pub(crate) fn is_request_candidate(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.starts_with("curl ")
        || trimmed.starts_with("curl\n")
        || trimmed.starts_with("curl\r")
        || trimmed.starts_with("curl.exe ")
        || trimmed == "curl"
    {
        return true;
    }
    if (trimmed.starts_with("http://") || trimmed.starts_with("https://"))
        && !trimmed.contains('\n')
        && !trimmed.contains('\r')
        && !trimmed.contains(' ')
    {
        return true;
    }
    if trimmed.starts_with("GET ")
        || trimmed.starts_with("POST ")
        || trimmed.starts_with("PUT ")
        || trimmed.starts_with("DELETE ")
        || trimmed.starts_with("PATCH ")
    {
        let mut parts = trimmed.split_whitespace();
        if let (Some(_), Some(u)) = (parts.next(), parts.next()) {
            if (u.starts_with("http://") || u.starts_with("https://")) && parts.next().is_none() {
                return true;
            }
        }
    }
    false
}

/// Try parsing text as a curl command or HTTP request, execute it, and return JSON if response Content-Type is JSON.
pub(crate) fn try_request_json(text: &str) -> Option<(Value, String)> {
    let trimmed = text.trim();
    let req = if trimmed.starts_with("curl") {
        let args = shlex::split(trimmed)?;
        parse_curl_command(&args)?
    } else if trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("GET ")
        || trimmed.starts_with("POST ")
    {
        parse_http_url(trimmed)?
    } else {
        return None;
    };

    let mut ureq_builder = match req.method.as_str() {
        "POST" => ureq::post(&req.url),
        "PUT" => ureq::put(&req.url),
        "DELETE" => ureq::delete(&req.url),
        "PATCH" => ureq::patch(&req.url),
        "HEAD" => ureq::head(&req.url),
        _ => ureq::get(&req.url),
    };

    ureq_builder = ureq_builder.timeout(std::time::Duration::from_secs(12));

    let mut has_accept = false;
    let mut has_content_type = false;
    for (k, v) in &req.headers {
        if k.eq_ignore_ascii_case("accept") {
            has_accept = true;
        }
        if k.eq_ignore_ascii_case("content-type") {
            has_content_type = true;
        }
        ureq_builder = ureq_builder.set(k, v);
    }
    if !has_accept {
        ureq_builder = ureq_builder.set("Accept", "application/json, */*;q=0.8");
    }
    if !has_content_type && req.body.is_some() {
        if let Some(body) = &req.body {
            let b_trim = body.trim();
            if (b_trim.starts_with('{') && b_trim.ends_with('}'))
                || (b_trim.starts_with('[') && b_trim.ends_with(']'))
            {
                ureq_builder = ureq_builder.set("Content-Type", "application/json");
            } else {
                ureq_builder =
                    ureq_builder.set("Content-Type", "application/x-www-form-urlencoded");
            }
        }
    }

    let resp_result = if let Some(body) = &req.body {
        ureq_builder.send_string(body)
    } else {
        ureq_builder.call()
    };

    let response = match resp_result {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(_) => return None,
    };

    // Recognize response contentType. If not JSON format, directly fail.
    let ct = response.header("Content-Type");
    if !is_json_content_type(ct) {
        return None;
    }

    let mut body_bytes = Vec::new();
    response
        .into_reader()
        .take(25 * 1024 * 1024)
        .read_to_end(&mut body_bytes)
        .ok()?;

    let body_str = String::from_utf8(body_bytes).ok()?;
    let value = serde_json::from_str::<Value>(&body_str).ok()?;

    Some((value, body_str))
}
