use std::io::Read;

const MAX_BODY_BYTES: usize = 10 * 1024 * 1024;
const MAX_RESPONSE_BYTES: usize = 25 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
struct HttpRequest {
    method: String,
    target: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

fn parse_http_request(bytes: &[u8]) -> Result<HttpRequest, &'static str> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("incomplete headers")?;
    let head = std::str::from_utf8(&bytes[..header_end]).map_err(|_| "invalid headers")?;
    let mut lines = head.split("\r\n");
    let request_line = lines.next().ok_or("missing request line")?;
    let mut parts = request_line.split_whitespace();
    let method = parts.next().ok_or("missing method")?;
    let target = parts.next().ok_or("missing target")?;
    if parts.next() != Some("HTTP/1.1") {
        return Err("only HTTP/1.1 is supported");
    }
    if !target.starts_with('/') || target.len() > 4096 {
        return Err("invalid request target");
    }
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect::<Vec<_>>();
    let content_length = headers
        .iter()
        .find(|(key, _)| key == "content-length")
        .and_then(|(_, value)| value.parse::<usize>().ok())
        .unwrap_or(0);
    if content_length > 10 * 1024 * 1024 {
        return Err("request body too large");
    }
    let body_start = header_end + 4;
    if bytes.len() < body_start + content_length {
        return Err("incomplete body");
    }
    Ok(HttpRequest {
        method: method.to_owned(),
        target: target.to_owned(),
        headers,
        body: bytes[body_start..body_start + content_length].to_vec(),
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let secret_id = option_value(&args, "--secret-id").ok_or("missing --secret-id")?;
    let provider = option_value(&args, "--provider").ok_or("missing --provider")?;
    let listen = option_value(&args, "--listen").unwrap_or_else(|| "127.0.0.1:0".to_owned());
    let database = secrethub_storage::Database::open(database_path())?;
    let mut vault = secrethub_core::VaultService::open(database)?;
    let password = rpassword::prompt_password("Master Password: ")?;
    vault.unlock(&password)?;
    let metadata = vault
        .list_metadata()?
        .into_iter()
        .find(|item| item.id == secret_id || item.env_key == secret_id)
        .ok_or("secret not found")?;
    let value = zeroize::Zeroizing::new(vault.read_secret(&metadata.id)?);
    let endpoint = endpoint_for(&provider).ok_or("provider has no proxy endpoint")?;
    secrethub_proxy::EndpointPolicy::new(&provider).authorize(endpoint)?;
    let listener = std::net::TcpListener::bind(&listen)?;
    let address = listener.local_addr()?;
    let mut authority = secrethub_proxy::TokenAuthority::new();
    let grant = authority.issue(now_seconds(), 300);
    println!("SECRETHUB_PROXY_URL=http://{address}");
    println!("SECRETHUB_PROXY_TOKEN={}", grant.token);
    println!("SECRETHUB_PROXY_EXPIRES_AT={}", grant.expires_at);
    let authority = std::sync::Arc::new(std::sync::Mutex::new(authority));
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()?;
    for stream in listener.incoming() {
        let mut stream = stream?;
        stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
        stream.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
        let request = read_request(&mut stream)?;
        let response = forward_request(
            &client,
            &authority,
            &provider,
            endpoint,
            value.as_str(),
            request,
        );
        write_response(&mut stream, response);
    }
    Ok(())
}

fn forward_request(
    client: &reqwest::blocking::Client,
    authority: &std::sync::Arc<std::sync::Mutex<secrethub_proxy::TokenAuthority>>,
    provider: &str,
    endpoint: &str,
    value: &str,
    request: HttpRequest,
) -> (u16, Vec<(String, String)>, Vec<u8>) {
    let token = request
        .headers
        .iter()
        .find(|(key, _)| key == "authorization")
        .and_then(|(_, value)| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    let authorized = authority
        .lock()
        .map(|mut authority| authority.authorize(token, now_seconds()))
        .unwrap_or(false);
    if !authorized {
        return (
            401,
            vec![("content-type".into(), "text/plain; charset=utf-8".into())],
            b"proxy token invalid or expired".to_vec(),
        );
    }
    let Some(method) = request_method(&request.method) else {
        return (400, vec![], b"unsupported HTTP method".to_vec());
    };
    let url = format!("{endpoint}{}", request.target);
    let Ok(builder) = client.request(method, url).build() else {
        return (400, vec![], b"invalid upstream request".to_vec());
    };
    let auth_header = match provider {
        "anthropic" => ("x-api-key", value.to_owned()),
        "gemini" => ("x-goog-api-key", value.to_owned()),
        _ => ("authorization", format!("Bearer {value}")),
    };
    let mut request_builder = client
        .request(builder.method().clone(), builder.url().clone())
        .header(auth_header.0, auth_header.1);
    for (key, header_value) in &request.headers {
        if matches!(key.as_str(), "content-type" | "accept" | "user-agent") {
            request_builder = request_builder.header(key, header_value);
        }
    }
    let Ok(response) = request_builder.body(request.body).send() else {
        return (502, vec![], b"upstream network error".to_vec());
    };
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(key, value)| {
            value
                .to_str()
                .ok()
                .map(|value| (key.to_string(), value.to_owned()))
        })
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "content-type" | "cache-control" | "retry-after"
            )
        })
        .collect();
    let mut body = Vec::new();
    let mut limited = response.take((MAX_RESPONSE_BYTES + 1) as u64);
    if std::io::Read::read_to_end(&mut limited, &mut body).is_err()
        || body.len() > MAX_RESPONSE_BYTES
    {
        return (502, vec![], b"upstream response too large".to_vec());
    }
    (status, headers, body)
}

fn request_method(method: &str) -> Option<reqwest::Method> {
    match method {
        "GET" => Some(reqwest::Method::GET),
        "POST" => Some(reqwest::Method::POST),
        "PUT" => Some(reqwest::Method::PUT),
        "PATCH" => Some(reqwest::Method::PATCH),
        "DELETE" => Some(reqwest::Method::DELETE),
        _ => None,
    }
}

fn read_request(
    stream: &mut std::net::TcpStream,
) -> Result<HttpRequest, Box<dyn std::error::Error>> {
    let mut bytes = Vec::new();
    let mut buffer = [0u8; 8192];
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        bytes.extend_from_slice(&buffer[..read]);
        if bytes.len() > MAX_BODY_BYTES + 8192 {
            return Err("request too large".into());
        }
        if let Ok(request) = parse_http_request(&bytes) {
            return Ok(request);
        }
    }
    Err("incomplete request".into())
}

fn write_response(
    stream: &mut std::net::TcpStream,
    response: (u16, Vec<(String, String)>, Vec<u8>),
) {
    use std::io::Write;
    let (status, headers, body) = response;
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        502 => "Bad Gateway",
        _ => "Upstream",
    };
    let mut output = format!(
        "HTTP/1.1 {status} {reason}\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (key, value) in headers {
        output.push_str(&format!("{key}: {value}\r\n"));
    }
    output.push_str("\r\n");
    let _ = stream.write_all(output.as_bytes());
    let _ = stream.write_all(&body);
}

fn option_value(args: &[String], key: &str) -> Option<String> {
    args.iter()
        .position(|arg| arg == key)
        .and_then(|index| args.get(index + 1))
        .cloned()
}
fn database_path() -> std::path::PathBuf {
    std::env::var_os("SECRETHUB_DB_PATH")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join("com.secrethub.desktop")
                .join("vault.sqlite")
        })
}
fn endpoint_for(provider: &str) -> Option<&'static str> {
    match provider {
        "openai" => Some("https://api.openai.com"),
        "anthropic" => Some("https://api.anthropic.com"),
        "gemini" => Some("https://generativelanguage.googleapis.com"),
        "deepseek" => Some("https://api.deepseek.com"),
        "xai" => Some("https://api.x.ai"),
        "openrouter" => Some("https://openrouter.ai"),
        "github" => Some("https://api.github.com"),
        "supabase" => Some("https://api.supabase.com"),
        "cloudflare" => Some("https://api.cloudflare.com"),
        _ => None,
    }
}
fn now_seconds() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[cfg(test)]
mod tests {
    use super::parse_http_request;

    #[test]
    fn parses_bounded_http_request_without_accepting_absolute_targets() {
        let request = parse_http_request(b"POST /v1/responses HTTP/1.1\r\nContent-Type: application/json\r\nContent-Length: 7\r\n\r\nfixture").unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/v1/responses");
        assert_eq!(request.body, b"fixture");
        assert!(parse_http_request(b"GET https://attacker.example HTTP/1.1\r\n\r\n").is_err());
        assert!(super::request_method("CONNECT").is_none());
    }

    #[test]
    fn rejects_missing_proxy_token_before_contacting_upstream() {
        let client = reqwest::blocking::Client::builder().build().unwrap();
        let authority =
            std::sync::Arc::new(std::sync::Mutex::new(secrethub_proxy::TokenAuthority::new()));
        let response = super::forward_request(
            &client,
            &authority,
            "openai",
            "https://api.openai.com",
            "fixture-only-value",
            super::HttpRequest {
                method: "GET".into(),
                target: "/v1/models".into(),
                headers: Vec::new(),
                body: Vec::new(),
            },
        );
        assert_eq!(response.0, 401);
        assert!(!String::from_utf8_lossy(&response.2).contains("fixture-only-value"));
    }
}
