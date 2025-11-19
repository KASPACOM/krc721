#[derive(Debug, Clone)]
pub enum Events {
    Stdout { line: String },
}
