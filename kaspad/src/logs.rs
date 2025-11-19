pub enum Log {
    Debug(String),
    Trace(String),
    Info(String),
    Warning(String),
    Error(String),
    Processed(String),
}

impl From<&str> for Log {
    fn from(line: &str) -> Self {
        let line = line.trim();
        if !line.is_empty() {
            if line.len() < 38 || &line[30..31] != "[" {
                Log::Info(line.to_string())
            } else {
                let time = &line[11..23];
                let kind = &line[31..36];
                let text = &line[38..];
                // text
                match kind {
                    "WARN " => Log::Warning(format!("{time} {text}")),
                    "ERROR" => Log::Error(format!("{time} {text}")),
                    _ => {
                        if text.starts_with("Processed") {
                            Log::Processed(format!("{time} {text}"))
                        } else {
                            Log::Info(format!("{time} {text}"))
                        }
                    }
                }
            }
        } else {
            Log::Info(line.to_string())
        }
    }
}

impl std::fmt::Display for Log {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let text = match self {
            Log::Info(text) => text.to_owned(),
            Log::Error(text) => text.to_owned(),
            Log::Warning(text) => text.to_owned(),
            Log::Debug(text) => text.to_owned(),
            Log::Trace(text) => text.to_owned(),
            Log::Processed(text) => text.to_owned(),
        };

        write!(f, "{}", text)
    }
}
