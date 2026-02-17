// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::{AppCommand, RssPreviewItem};
use crate::config::{get_watch_path, RssAddedVia, RssHistoryEntry, Settings};
use crate::persistence::rss::load_rss_state;
use chrono::{Duration as ChronoDuration, Utc};
use feed_rs::parser;
use regex::Regex;
use reqwest::Client;
use sha1::{Digest, Sha1};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tokio::sync::{broadcast, mpsc};
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

const MIN_POLL_INTERVAL_SECS: u64 = 30;
const MAX_TORRENT_DOWNLOAD_BYTES: usize = 10 * 1024 * 1024;
const REQUEST_TIMEOUT_SECS: u64 = 20;
const FEED_FETCH_MAX_ATTEMPTS: u32 = 3;
const FEED_RETRY_BASE_DELAY_MS: u64 = 400;
const FEED_RETRY_MAX_JITTER_MS: u64 = 250;

#[derive(Clone)]
struct CandidateItem {
    dedupe_key: String,
    title: String,
    link: Option<String>,
    guid: Option<String>,
    source: Option<String>,
    date_iso: Option<String>,
    sort_ts: i64,
}

pub fn spawn_rss_service(
    settings: Settings,
    app_command_tx: mpsc::Sender<AppCommand>,
    mut sync_now_rx: mpsc::Receiver<()>,
    mut settings_rx: tokio::sync::watch::Receiver<Settings>,
    shutdown_tx: broadcast::Sender<()>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut shutdown_rx = shutdown_tx.subscribe();
        let mut current_settings = settings;
        let mut poll_secs = current_settings
            .rss
            .poll_interval_secs
            .max(MIN_POLL_INTERVAL_SECS);
        let mut ticker = time::interval(Duration::from_secs(poll_secs));
        ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);

        let client = match Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
        {
            Ok(c) => c,
            Err(_) => return,
        };

        let mut downloaded_keys: HashSet<String> = load_rss_state()
            .history
            .into_iter()
            .map(|h| h.dedupe_key)
            .collect();

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    break;
                }
                changed = settings_rx.changed() => {
                    if changed.is_err() {
                        break;
                    }
                    current_settings = settings_rx.borrow().clone();
                    poll_secs = current_settings
                        .rss
                        .poll_interval_secs
                        .max(MIN_POLL_INTERVAL_SECS);
                    ticker = time::interval(Duration::from_secs(poll_secs));
                    ticker.set_missed_tick_behavior(time::MissedTickBehavior::Delay);
                }
                maybe_sync = sync_now_rx.recv() => {
                    if maybe_sync.is_none() {
                        break;
                    }
                    if !current_settings.rss.enabled {
                        continue;
                    }
                    run_sync(&current_settings, &client, &app_command_tx, &mut downloaded_keys).await;
                    let now = Utc::now();
                    let next = now + ChronoDuration::seconds(poll_secs as i64);
                    let _ = app_command_tx.send(AppCommand::RssSyncStatusUpdated {
                        last_sync_at: Some(now.to_rfc3339()),
                        next_sync_at: Some(next.to_rfc3339()),
                    }).await;
                }
                _ = ticker.tick() => {
                    if !current_settings.rss.enabled {
                        continue;
                    }
                    run_sync(&current_settings, &client, &app_command_tx, &mut downloaded_keys).await;
                    let now = Utc::now();
                    let next = now + ChronoDuration::seconds(poll_secs as i64);
                    let _ = app_command_tx.send(AppCommand::RssSyncStatusUpdated {
                        last_sync_at: Some(now.to_rfc3339()),
                        next_sync_at: Some(next.to_rfc3339()),
                    }).await;
                }
            }
        }
    })
}

pub async fn manual_ingest_preview_item(
    settings: &Settings,
    item: &RssPreviewItem,
) -> Result<RssHistoryEntry, String> {
    let link = item
        .link
        .as_ref()
        .ok_or_else(|| "selected item has no link".to_string())?;

    if link.starts_with("magnet:") {
        write_magnet(settings, link).map_err(|e| format!("magnet write failed: {e}"))?;
    } else if link.starts_with("http://") || link.starts_with("https://") {
        let client = Client::builder()
            .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
            .build()
            .map_err(|e| format!("HTTP client build failed: {e}"))?;
        let bytes = fetch_torrent_bytes(&client, link).await?;
        write_torrent_bytes(settings, link, &bytes)
            .map_err(|e| format!("torrent write failed: {e}"))?;
    } else {
        return Err("unsupported link scheme".to_string());
    }

    Ok(RssHistoryEntry {
        dedupe_key: item.dedupe_key.clone(),
        guid: item.guid.clone(),
        link: item.link.clone(),
        title: item.title.clone(),
        source: item.source.clone(),
        date_iso: item
            .date_iso
            .clone()
            .unwrap_or_else(|| Utc::now().to_rfc3339()),
        added_via: RssAddedVia::Manual,
    })
}

async fn run_sync(
    settings: &Settings,
    client: &Client,
    app_command_tx: &mpsc::Sender<AppCommand>,
    downloaded_keys: &mut HashSet<String>,
) {
    let enabled_feeds: Vec<_> = settings.rss.feeds.iter().filter(|f| f.enabled).collect();
    if enabled_feeds.is_empty() {
        let _ = app_command_tx
            .send(AppCommand::RssPreviewUpdated(Vec::new()))
            .await;
        return;
    }

    let filter_regexes = compile_filters(settings);

    let mut aggregated = Vec::new();

    for feed in enabled_feeds {
        match fetch_and_parse_feed_with_retry(client, &feed.url, FEED_FETCH_MAX_ATTEMPTS).await {
            Ok(mut items) => {
                let _ = app_command_tx
                    .send(AppCommand::RssFeedErrorUpdated {
                        feed_url: feed.url.clone(),
                        error: None,
                    })
                    .await;
                aggregated.append(&mut items);
            }
            Err(e) => {
                let _ = app_command_tx
                    .send(AppCommand::RssFeedErrorUpdated {
                        feed_url: feed.url.clone(),
                        error: Some(crate::config::FeedSyncError {
                            message: e,
                            occurred_at_iso: Utc::now().to_rfc3339(),
                        }),
                    })
                    .await;
            }
        }
    }

    aggregated.sort_by(|a, b| b.sort_ts.cmp(&a.sort_ts));

    let mut title_seen = HashSet::new();
    let mut preview_items = Vec::new();

    for item in aggregated {
        let title_key = normalize_title(&item.title);
        if !title_seen.insert(title_key) {
            continue;
        }

        let is_match = filter_regexes
            .iter()
            .any(|regex| regex.is_match(item.title.as_str()));
        let mut is_downloaded = downloaded_keys.contains(&item.dedupe_key);

        if is_match && !is_downloaded {
            let added = auto_ingest_item(settings, client, &item).await;
            if added {
                is_downloaded = true;
                downloaded_keys.insert(item.dedupe_key.clone());

                let entry = RssHistoryEntry {
                    dedupe_key: item.dedupe_key.clone(),
                    guid: item.guid.clone(),
                    link: item.link.clone(),
                    title: item.title.clone(),
                    source: item.source.clone(),
                    date_iso: item
                        .date_iso
                        .clone()
                        .unwrap_or_else(|| Utc::now().to_rfc3339()),
                    added_via: RssAddedVia::Auto,
                };

                let _ = app_command_tx
                    .send(AppCommand::RssDownloadSelected(entry))
                    .await;
            }
        }

        preview_items.push(RssPreviewItem {
            dedupe_key: item.dedupe_key,
            title: item.title,
            link: item.link,
            guid: item.guid,
            source: item.source,
            date_iso: item.date_iso,
            is_match,
            is_downloaded,
        });

        if preview_items.len() >= settings.rss.max_preview_items {
            break;
        }
    }

    let _ = app_command_tx
        .send(AppCommand::RssPreviewUpdated(preview_items))
        .await;
}

fn compile_filters(settings: &Settings) -> Vec<Regex> {
    settings
        .rss
        .filters
        .iter()
        .filter(|f| f.enabled)
        .filter_map(|filter| Regex::new(filter.regex.as_str()).ok())
        .collect()
}

async fn fetch_and_parse_feed(
    client: &Client,
    feed_url: &str,
) -> Result<Vec<CandidateItem>, String> {
    let response = client
        .get(feed_url)
        .send()
        .await
        .map_err(|e| format!("feed request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("feed HTTP status {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("feed body read failed: {e}"))?;

    let feed = parser::parse(bytes.as_ref()).map_err(|e| format!("feed parse failed: {e}"))?;
    let source_name = feed
        .title
        .as_ref()
        .map(|t| t.content.clone())
        .filter(|s| !s.trim().is_empty());

    let mut out = Vec::new();
    for entry in feed.entries {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_else(|| "Untitled".to_string());

        let link = entry.links.iter().find_map(|l| {
            if l.href.trim().is_empty() {
                None
            } else {
                Some(l.href.clone())
            }
        });

        let guid = if entry.id.trim().is_empty() {
            None
        } else {
            Some(entry.id.clone())
        };

        let published = entry
            .published
            .or(entry.updated)
            .map(|dt| dt.with_timezone(&Utc));

        let dedupe_key = dedupe_key_for(
            guid.as_deref(),
            link.as_deref(),
            title.as_str(),
            source_name.as_deref(),
        );

        out.push(CandidateItem {
            dedupe_key,
            title,
            link,
            guid,
            source: source_name.clone(),
            date_iso: published.map(|dt| dt.to_rfc3339()),
            sort_ts: published.map(|dt| dt.timestamp()).unwrap_or(0),
        });
    }

    Ok(out)
}

fn retry_delay_ms(feed_url: &str, attempt_index: u32) -> u64 {
    let digest = Sha1::digest(format!("{feed_url}:{attempt_index}").as_bytes());
    let jitter =
        (u16::from_le_bytes([digest[0], digest[1]]) as u64) % (FEED_RETRY_MAX_JITTER_MS + 1);
    let exponential = FEED_RETRY_BASE_DELAY_MS * (1u64 << attempt_index.min(4));
    exponential + jitter
}

async fn fetch_and_parse_feed_with_retry(
    client: &Client,
    feed_url: &str,
    max_attempts: u32,
) -> Result<Vec<CandidateItem>, String> {
    let attempts = max_attempts.max(1);
    let mut last_error: Option<String> = None;

    for attempt in 1..=attempts {
        match fetch_and_parse_feed(client, feed_url).await {
            Ok(items) => return Ok(items),
            Err(err) => {
                last_error = Some(err);
                if attempt < attempts {
                    let delay_ms = retry_delay_ms(feed_url, attempt - 1);
                    time::sleep(Duration::from_millis(delay_ms)).await;
                }
            }
        }
    }

    Err(format!(
        "feed sync failed after {} attempts: {}",
        attempts,
        last_error.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn dedupe_key_for(
    guid: Option<&str>,
    link: Option<&str>,
    title: &str,
    source: Option<&str>,
) -> String {
    if let Some(g) = guid.filter(|v| !v.trim().is_empty()) {
        return format!("guid:{}", g.trim());
    }
    if let Some(l) = link.filter(|v| !v.trim().is_empty()) {
        return format!("link:{}", l.trim());
    }

    let normalized_title = normalize_title(title);
    let normalized_source = normalize_title(source.unwrap_or(""));
    format!("title_source:{}::{}", normalized_title, normalized_source)
}

fn normalize_title(input: &str) -> String {
    input
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

async fn auto_ingest_item(settings: &Settings, client: &Client, item: &CandidateItem) -> bool {
    let Some(link) = &item.link else {
        return false;
    };

    if link.starts_with("magnet:") {
        return write_magnet(settings, link.as_str()).is_ok();
    }

    if !(link.starts_with("http://") || link.starts_with("https://")) {
        return false;
    }

    match fetch_torrent_bytes(client, link).await {
        Ok(bytes) => write_torrent_bytes(settings, link.as_str(), &bytes).is_ok(),
        Err(_) => false,
    }
}

async fn fetch_torrent_bytes(client: &Client, url: &str) -> Result<Vec<u8>, String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("torrent request failed: {e}"))?;

    if !response.status().is_success() {
        return Err(format!("torrent HTTP status {}", response.status()));
    }

    let bytes = response
        .bytes()
        .await
        .map_err(|e| format!("torrent body read failed: {e}"))?;

    if bytes.len() > MAX_TORRENT_DOWNLOAD_BYTES {
        return Err("torrent payload exceeds max allowed size".to_string());
    }

    Ok(bytes.to_vec())
}

fn write_magnet(settings: &Settings, magnet_link: &str) -> io::Result<PathBuf> {
    let watch_dir = rss_watch_dir(settings)?;
    let hash = hex::encode(Sha1::digest(magnet_link.as_bytes()));
    let final_path = watch_dir.join(format!("{}.magnet", hash));
    let temp_path = watch_dir.join(format!("{}.magnet.tmp", hash));

    atomic_write(&temp_path, &final_path, magnet_link.as_bytes())?;
    Ok(final_path)
}

fn write_torrent_bytes(settings: &Settings, source_url: &str, bytes: &[u8]) -> io::Result<PathBuf> {
    let watch_dir = rss_watch_dir(settings)?;
    let hash = hex::encode(Sha1::digest(source_url.as_bytes()));
    let final_path = watch_dir.join(format!("{}.torrent", hash));
    let temp_path = watch_dir.join(format!("{}.torrent.tmp", hash));

    atomic_write(&temp_path, &final_path, bytes)?;
    Ok(final_path)
}

fn atomic_write(temp_path: &Path, final_path: &Path, payload: &[u8]) -> io::Result<()> {
    if let Some(parent) = final_path.parent() {
        fs::create_dir_all(parent)?;
    }

    fs::write(temp_path, payload)?;
    fs::rename(temp_path, final_path)?;
    Ok(())
}

fn rss_watch_dir(settings: &Settings) -> io::Result<PathBuf> {
    if let Some(path) = settings.watch_folder.clone() {
        return Ok(path);
    }

    let (watch_path, _) = get_watch_path().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "watch path unavailable for RSS auto-ingest",
        )
    })?;
    Ok(watch_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn dedupe_key_prefers_guid_then_link_then_title_source() {
        let a = dedupe_key_for(Some("guid-1"), Some("https://x"), "Title", Some("Feed"));
        assert_eq!(a, "guid:guid-1");

        let b = dedupe_key_for(None, Some("https://x"), "Title", Some("Feed"));
        assert_eq!(b, "link:https://x");

        let c = dedupe_key_for(None, None, "Title  One", Some("Feed  A"));
        assert_eq!(c, "title_source:title one::feed a");
    }

    #[test]
    fn normalize_title_compacts_whitespace_and_case() {
        assert_eq!(normalize_title("  Ubuntu   ISO  "), "ubuntu iso");
    }

    #[test]
    fn retry_delay_has_jitter_and_increases_with_attempt() {
        let first = retry_delay_ms("https://example.test/rss.xml", 0);
        let second = retry_delay_ms("https://example.test/rss.xml", 1);

        assert!(first >= FEED_RETRY_BASE_DELAY_MS);
        assert!(first <= FEED_RETRY_BASE_DELAY_MS + FEED_RETRY_MAX_JITTER_MS);
        assert!(second >= FEED_RETRY_BASE_DELAY_MS * 2);
        assert!(second <= FEED_RETRY_BASE_DELAY_MS * 2 + FEED_RETRY_MAX_JITTER_MS);
    }

    #[test]
    fn retry_delay_is_deterministic_for_same_input() {
        let a = retry_delay_ms("https://example.test/rss.xml", 2);
        let b = retry_delay_ms("https://example.test/rss.xml", 2);
        assert_eq!(a, b);
    }

    #[tokio::test]
    async fn rss_service_disabled_waits_for_shutdown() {
        let settings = Settings::default();
        let (tx, mut rx) = mpsc::channel::<AppCommand>(2);
        let (sync_tx, sync_rx) = mpsc::channel::<()>(2);
        let (settings_tx, settings_rx) = tokio::sync::watch::channel(settings.clone());
        let (shutdown_tx, _) = broadcast::channel(1);

        let handle = spawn_rss_service(settings, tx, sync_rx, settings_rx, shutdown_tx.clone());
        drop(sync_tx);
        drop(settings_tx);
        tokio::task::yield_now().await;

        let _ = shutdown_tx.send(());
        let join_result = tokio::time::timeout(Duration::from_secs(2), handle).await;
        assert!(join_result.is_ok());

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn rss_service_applies_runtime_settings_update_on_sync_now() {
        let settings = Settings::default();
        let (tx, mut rx) = mpsc::channel::<AppCommand>(8);
        let (sync_tx, sync_rx) = mpsc::channel::<()>(2);
        let (settings_tx, settings_rx) = tokio::sync::watch::channel(settings.clone());
        let (shutdown_tx, _) = broadcast::channel(1);

        let handle = spawn_rss_service(settings, tx, sync_rx, settings_rx, shutdown_tx.clone());
        tokio::task::yield_now().await;

        // Enable RSS at runtime with no feeds (network-free path):
        // run_sync should emit RssPreviewUpdated(Vec::new()).
        let mut updated = Settings::default();
        updated.rss.enabled = true;
        settings_tx.send(updated).expect("send settings update");
        sync_tx.send(()).await.expect("send sync trigger");

        let got = tokio::time::timeout(Duration::from_secs(2), rx.recv())
            .await
            .expect("timed out waiting for command");
        match got {
            Some(AppCommand::RssPreviewUpdated(items)) => assert!(items.is_empty()),
            other => panic!("unexpected command: {:?}", other.map(|_| "non-preview")),
        }

        let _ = shutdown_tx.send(());
        let _ = tokio::time::timeout(Duration::from_secs(2), handle).await;
    }

    #[tokio::test]
    async fn manual_ingest_magnet_writes_watch_file_and_returns_history_entry() {
        let tmp = tempdir().expect("tempdir");
        let mut settings = Settings::default();
        settings.watch_folder = Some(tmp.path().to_path_buf());

        let magnet = "magnet:?xt=urn:btih:00112233445566778899AABBCCDDEEFF00112233";
        let item = RssPreviewItem {
            dedupe_key: "guid:abc123".to_string(),
            title: "Ubuntu ISO".to_string(),
            link: Some(magnet.to_string()),
            guid: Some("abc123".to_string()),
            source: Some("Example Feed".to_string()),
            date_iso: Some("2026-02-17T12:00:00Z".to_string()),
            ..Default::default()
        };

        let entry = manual_ingest_preview_item(&settings, &item)
            .await
            .expect("manual ingest should succeed");

        let expected_name = format!("{}.magnet", hex::encode(Sha1::digest(magnet.as_bytes())));
        let expected_path = tmp.path().join(expected_name);
        assert!(expected_path.exists(), "expected magnet file to be created");

        let content = std::fs::read_to_string(expected_path).expect("read magnet file");
        assert_eq!(content, magnet);

        assert_eq!(entry.dedupe_key, "guid:abc123");
        assert_eq!(entry.title, "Ubuntu ISO");
        assert_eq!(entry.guid.as_deref(), Some("abc123"));
        assert_eq!(entry.source.as_deref(), Some("Example Feed"));
        assert_eq!(entry.added_via, RssAddedVia::Manual);
    }

    #[tokio::test]
    async fn manual_ingest_http_torrent_writes_watch_file_and_returns_history_entry() {
        let tmp = tempdir().expect("tempdir");
        let mut settings = Settings::default();
        settings.watch_folder = Some(tmp.path().to_path_buf());

        let payload = b"d8:announce13:http://t4:infod6:lengthi1e4:name4:test12:piece lengthi1e6:pieces20:aaaaaaaaaaaaaaaaaaaaee";
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind local test server");
        let addr = listener.local_addr().expect("local addr");

        let server_task = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.expect("accept");
            let mut buf = [0u8; 1024];
            let _ = socket.read(&mut buf).await;
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/x-bittorrent\r\nConnection: close\r\n\r\n",
                payload.len()
            );
            socket
                .write_all(response.as_bytes())
                .await
                .expect("write headers");
            socket.write_all(payload).await.expect("write body");
        });

        let url = format!("http://{}/example.torrent", addr);
        let item = RssPreviewItem {
            dedupe_key: "link:test-http".to_string(),
            title: "Fedora ISO".to_string(),
            link: Some(url.clone()),
            guid: None,
            source: Some("HTTP Feed".to_string()),
            date_iso: Some("2026-02-17T13:00:00Z".to_string()),
            ..Default::default()
        };

        let entry = manual_ingest_preview_item(&settings, &item)
            .await
            .expect("manual ingest should succeed");

        server_task.await.expect("server task");

        let expected_name = format!("{}.torrent", hex::encode(Sha1::digest(url.as_bytes())));
        let expected_path = tmp.path().join(expected_name);
        assert!(
            expected_path.exists(),
            "expected torrent file to be created"
        );

        let content = std::fs::read(expected_path).expect("read torrent file");
        assert_eq!(content, payload);

        assert_eq!(entry.dedupe_key, "link:test-http");
        assert_eq!(entry.title, "Fedora ISO");
        assert_eq!(entry.source.as_deref(), Some("HTTP Feed"));
        assert_eq!(entry.added_via, RssAddedVia::Manual);
    }
}
