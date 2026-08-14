pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("visual element error: {0}")]
    VisualElementError(String),

    #[error("font load error: {0}")]
    FontLoadError(String),

    #[error("layout error: {0}")]
    LayoutError(String),

    #[error("css parse error: {0}")]
    CssParseError(String),

    #[error("html parse error: {0}")]
    HtmlError(String),

    #[error("render error: {0}")]
    RenderError(String),

    #[error("io error: {0}")]
    IoError(#[from] std::io::Error),
}
