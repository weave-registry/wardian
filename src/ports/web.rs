//! Fetching a zip from a link the user pasted.

pub trait Downloader: Send + Sync {
    /// The body of `url`, read up to `limit + 1` bytes so the caller can tell it was too large.
    /// Internal addresses (this machine, cloud metadata, and the private network unless allowed)
    /// are refused, redirects included.
    fn fetch(&self, url: &str, limit: u64) -> Result<Vec<u8>, String>;
}
