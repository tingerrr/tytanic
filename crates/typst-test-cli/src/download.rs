// SPDX-License-Identifier: Apache-2.0
// Credits: The Typst Authors

// Acknowledgement:
// Closely modelled after rustup's `DownloadTracker`.
// https://github.com/rust-lang/rustup/blob/master/src/cli/download_tracker.rs

// TODO: use typst-kit
#![allow(dead_code)]

use std::collections::VecDeque;
use std::fmt::Debug;
use std::io;
use std::io::{ErrorKind, Read};
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use native_tls::{Certificate, TlsConnector};
use ureq::Response;

use crate::report::Reporter;

/// Keep track of this many download speed samples.
const SPEED_SAMPLES: usize = 5;

pub struct Downloader {
    cert_path: Option<PathBuf>,
    cert: OnceLock<Option<Certificate>>,
}

impl Debug for Downloader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Downloader")
            .field("cert_path", &self.cert_path)
            .finish_non_exhaustive()
    }
}

impl Downloader {
    pub fn new(cert_path: Option<PathBuf>) -> Self {
        Self {
            cert_path,
            cert: OnceLock::new(),
        }
    }

    fn get_cert(&self) -> Option<&Certificate> {
        self.cert
            .get_or_init(|| {
                let path = self.cert_path.clone()?;
                let pem = std::fs::read(path).ok()?;
                Certificate::from_pem(&pem).ok()
            })
            .as_ref()
    }

    /// Download binary data and display its progress.
    #[allow(clippy::result_large_err)]
    pub fn download_with_progress(
        &self,
        url: &str,
        // reporter: &mut Reporter,
    ) -> Result<Vec<u8>, ureq::Error> {
        let response = self.download(url)?;
        Ok(RemoteReader::from_response(response).download()?)
    }

    /// Download from a URL.
    #[allow(clippy::result_large_err)]
    pub fn download(&self, url: &str) -> Result<ureq::Response, ureq::Error> {
        let mut builder = ureq::AgentBuilder::new();
        let mut tls = TlsConnector::builder();

        builder = builder.user_agent(concat!("typst-test/", env!("CARGO_PKG_VERSION")));

        if let Some(proxy) = env_proxy::for_url_str(url)
            .to_url()
            .and_then(|url| ureq::Proxy::new(url).ok())
        {
            builder = builder.proxy(proxy);
        }

        if let Some(cert) = self.get_cert() {
            tls.add_root_certificate(cert.clone());
        }

        let connector = tls
            .build()
            .map_err(|err| io::Error::new(io::ErrorKind::Other, err))?;
        builder = builder.tls_connector(Arc::new(connector));

        builder.build().get(url).call()
    }
}

/// A wrapper around [`ureq::Response`] that reads the response body in chunks
/// over a websocket and displays statistics about its progress.
///
/// Downloads will _never_ fail due to statistics failing to print, print errors
/// are silently ignored.
struct RemoteReader {
    reader: Box<dyn Read + Send + Sync + 'static>,
    content_len: Option<usize>,
    total_downloaded: usize,
    downloaded_this_sec: usize,
    downloaded_last_few_secs: VecDeque<usize>,
    start_time: Instant,
    last_print: Option<Instant>,
}

impl RemoteReader {
    /// Wraps a [`ureq::Response`] and prepares it for downloading.
    ///
    /// The 'Content-Length' header is used as a size hint for read
    /// optimization, if present.
    pub fn from_response(response: Response) -> Self {
        let content_len: Option<usize> = response
            .header("Content-Length")
            .and_then(|header| header.parse().ok());

        Self {
            reader: response.into_reader(),
            content_len,
            total_downloaded: 0,
            downloaded_this_sec: 0,
            downloaded_last_few_secs: VecDeque::with_capacity(SPEED_SAMPLES),
            start_time: Instant::now(),
            last_print: None,
        }
    }

    /// Download the bodies content as raw bytes while attempting to print
    /// download statistics to standard error. Download progress gets displayed
    /// and updated every second.
    ///
    /// These statistics will never prevent a download from completing, errors
    /// are silently ignored.
    pub fn download(mut self) -> io::Result<Vec<u8>> {
        let mut buffer = vec![0; 8192];
        let mut data = match self.content_len {
            Some(content_len) => Vec::with_capacity(content_len),
            None => Vec::with_capacity(8192),
        };

        loop {
            let read = match self.reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(n) => n,
                // If the data is not yet ready but will be available eventually
                // keep trying until we either get an actual error, receive data
                // or an Ok(0).
                Err(e) if e.kind() == ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            };

            data.extend(&buffer[..read]);

            let last_printed = match self.last_print {
                Some(prev) => prev,
                None => {
                    let current_time = Instant::now();
                    self.last_print = Some(current_time);
                    current_time
                }
            };
            let elapsed = Instant::now().saturating_duration_since(last_printed);

            self.total_downloaded += read;
            self.downloaded_this_sec += read;

            if elapsed >= Duration::from_secs(1) {
                if self.downloaded_last_few_secs.len() == SPEED_SAMPLES {
                    self.downloaded_last_few_secs.pop_back();
                }

                self.downloaded_last_few_secs
                    .push_front(self.downloaded_this_sec);
                self.downloaded_this_sec = 0;

                // reporter.clear_last_lines(1)?;
                // self.display(reporter)?;
                self.last_print = Some(Instant::now());
            }
        }

        // self.display(reporter)?;
        // writeln!(reporter)?;

        Ok(data)
    }

    /// Compile and format several download statistics and make an attempt at
    /// displaying them on standard error.
    fn display(&mut self, reporter: &mut Reporter) -> io::Result<()> {
        let sum: usize = self.downloaded_last_few_secs.iter().sum();
        let len = self.downloaded_last_few_secs.len();
        let speed = if len > 0 {
            sum / len
        } else {
            self.content_len.unwrap_or(0)
        };

        let total_downloaded = as_bytes_unit(self.total_downloaded);
        let speed_h = as_throughput_unit(speed);
        let elapsed = time_suffix(Instant::now().saturating_duration_since(self.start_time));

        match self.content_len {
            Some(content_len) => {
                let percent = (self.total_downloaded as f64 / content_len as f64) * 100.;
                let remaining = content_len - self.total_downloaded;

                let download_size = as_bytes_unit(content_len);
                let eta = time_suffix(Duration::from_secs(if speed == 0 {
                    0
                } else {
                    (remaining / speed) as u64
                }));
                reporter.hint(
                    format!("{total_downloaded} / {download_size} ({percent:3.0} %) {speed_h} in {elapsed} ETA: {eta}"),
                )?;
            }
            None => reporter.hint(format!(
                "Total downloaded: {total_downloaded} Speed: {speed_h} Elapsed: {elapsed}"
            ))?,
        };
        Ok(())
    }
}

/// Append a unit-of-time suffix.
fn time_suffix(duration: Duration) -> String {
    let secs = duration.as_secs();
    match format_dhms(secs) {
        (0, 0, 0, s) => format!("{s:2.0}s"),
        (0, 0, m, s) => format!("{m:2.0}m {s:2.0}s"),
        (0, h, m, s) => format!("{h:2.0}h {m:2.0}m {s:2.0}s"),
        (d, h, m, s) => format!("{d:3.0}d {h:2.0}h {m:2.0}m {s:2.0}s"),
    }
}

/// Format the total amount of seconds into the amount of days, hours, minutes
/// and seconds.
fn format_dhms(sec: u64) -> (u64, u8, u8, u8) {
    let (mins, sec) = (sec / 60, (sec % 60) as u8);
    let (hours, mins) = (mins / 60, (mins % 60) as u8);
    let (days, hours) = (hours / 24, (hours % 24) as u8);
    (days, hours, mins, sec)
}

/// Format a given size as a unit of time.
fn as_bytes_unit(size: usize) -> String {
    const KI: f64 = 1024.0;
    const MI: f64 = KI * KI;
    const GI: f64 = KI * KI * KI;

    let size = size as f64;

    if size >= GI {
        format!("{:5.1} GiB", size / GI)
    } else if size >= MI {
        format!("{:5.1} MiB", size / MI)
    } else if size >= KI {
        format!("{:5.1} KiB", size / KI)
    } else {
        format!("{size:3.0} B")
    }
}

fn as_throughput_unit(size: usize) -> String {
    as_bytes_unit(size) + "/s"
}
