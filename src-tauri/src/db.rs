use std::{collections::HashMap, path::Path};

use anyhow::Result;
use chrono::{Duration, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions},
    Row, SqlitePool,
};

use crate::models::{
    Device, DiscoveredDevice, ProbeHit, Service, Settings, DEFAULT_DASHBOARD_PORT,
    LEGACY_DASHBOARD_PORT,
};

pub async fn connect(path: &Path) -> Result<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(options)
        .await?;

    migrate(&pool).await?;
    Ok(pool)
}

async fn migrate(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS devices (
            id TEXT PRIMARY KEY,
            ip TEXT NOT NULL,
            hostname TEXT,
            mac TEXT,
            vendor TEXT,
            name_override TEXT,
            selected INTEGER NOT NULL DEFAULT 0,
            ignored INTEGER NOT NULL DEFAULT 0,
            source TEXT NOT NULL,
            last_seen TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("ALTER TABLE devices ADD COLUMN vendor TEXT")
        .execute(pool)
        .await
        .ok();
    sqlx::query("ALTER TABLE devices ADD COLUMN name_override TEXT")
        .execute(pool)
        .await
        .ok();

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS favorites (
            service_key TEXT PRIMARY KEY,
            created_at TEXT NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS services (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            device_id TEXT NOT NULL,
            ip TEXT NOT NULL,
            port INTEGER NOT NULL,
            scheme TEXT NOT NULL,
            url TEXT NOT NULL,
            title TEXT,
            status_code INTEGER,
            server TEXT,
            first_seen TEXT NOT NULL,
            last_seen TEXT NOT NULL,
            last_checked TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1,
            last_failure TEXT,
            UNIQUE(device_id, port),
            FOREIGN KEY(device_id) REFERENCES devices(id) ON DELETE CASCADE
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS scan_history (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            started_at TEXT NOT NULL,
            finished_at TEXT NOT NULL,
            selected_devices INTEGER NOT NULL,
            discovered_services INTEGER NOT NULL
        );
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn load_settings(pool: &SqlitePool) -> Result<Settings> {
    let rows = sqlx::query("SELECT key, value FROM settings")
        .fetch_all(pool)
        .await?;
    let values: HashMap<String, String> = rows
        .into_iter()
        .map(|row| (row.get::<String, _>("key"), row.get::<String, _>("value")))
        .collect();

    let defaults = Settings::default();
    let stored_dashboard_port = values
        .get("dashboard_port")
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(defaults.dashboard_port);
    let dashboard_port_migrated = parse_bool(&values, "dashboard_port_migrated_from_8765", false);
    let dashboard_port =
        if !dashboard_port_migrated && stored_dashboard_port == LEGACY_DASHBOARD_PORT {
            DEFAULT_DASHBOARD_PORT
        } else {
            stored_dashboard_port
        };

    let settings = Settings {
        auto_scan: parse_bool(&values, "auto_scan", defaults.auto_scan),
        manual_only: parse_bool(&values, "manual_only", defaults.manual_only),
        minimize_to_tray: parse_bool(&values, "minimize_to_tray", defaults.minimize_to_tray),
        launch_at_startup: parse_bool(&values, "launch_at_startup", defaults.launch_at_startup),
        scan_interval_seconds: parse_u64(
            &values,
            "scan_interval_seconds",
            defaults.scan_interval_seconds,
        ),
        discovery_interval_seconds: parse_u64(
            &values,
            "discovery_interval_seconds",
            defaults.discovery_interval_seconds,
        ),
        retention_days: parse_i64(&values, "retention_days", defaults.retention_days),
        scan_concurrency: parse_usize(&values, "scan_concurrency", defaults.scan_concurrency),
        connect_timeout_ms: parse_u64(&values, "connect_timeout_ms", defaults.connect_timeout_ms),
        http_timeout_ms: parse_u64(&values, "http_timeout_ms", defaults.http_timeout_ms),
        dashboard_bind: values
            .get("dashboard_bind")
            .cloned()
            .unwrap_or(defaults.dashboard_bind),
        dashboard_port,
    }
    .normalized();

    save_settings(pool, &settings).await?;
    if !dashboard_port_migrated && stored_dashboard_port == LEGACY_DASHBOARD_PORT {
        save_setting(pool, "dashboard_port_migrated_from_8765", "true").await?;
    }
    Ok(settings)
}

pub async fn save_settings(pool: &SqlitePool, settings: &Settings) -> Result<()> {
    let pairs = [
        ("auto_scan", settings.auto_scan.to_string()),
        ("manual_only", settings.manual_only.to_string()),
        ("minimize_to_tray", settings.minimize_to_tray.to_string()),
        ("launch_at_startup", settings.launch_at_startup.to_string()),
        (
            "scan_interval_seconds",
            settings.scan_interval_seconds.to_string(),
        ),
        (
            "discovery_interval_seconds",
            settings.discovery_interval_seconds.to_string(),
        ),
        ("retention_days", settings.retention_days.to_string()),
        ("scan_concurrency", settings.scan_concurrency.to_string()),
        (
            "connect_timeout_ms",
            settings.connect_timeout_ms.to_string(),
        ),
        ("http_timeout_ms", settings.http_timeout_ms.to_string()),
        ("dashboard_bind", settings.dashboard_bind.clone()),
        ("dashboard_port", settings.dashboard_port.to_string()),
    ];

    for (key, value) in pairs {
        save_setting(pool, key, &value).await?;
    }

    Ok(())
}

async fn save_setting(pool: &SqlitePool, key: &str, value: &str) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO settings(key, value) VALUES (?, ?)
        ON CONFLICT(key) DO UPDATE SET value = excluded.value
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn upsert_device(pool: &SqlitePool, discovered: &DiscoveredDevice) -> Result<Device> {
    let now = Utc::now().to_rfc3339();
    let id = discovered.stable_id();

    if let Some(existing) = sqlx::query("SELECT id FROM devices WHERE ip = ? AND id != ?")
        .bind(&discovered.ip)
        .bind(&id)
        .fetch_optional(pool)
        .await?
    {
        let old_id: String = existing.get("id");
        sqlx::query("UPDATE services SET device_id = ? WHERE device_id = ?")
            .bind(&id)
            .bind(&old_id)
            .execute(pool)
            .await?;
        migrate_favorite_device_keys(pool, &old_id, &id).await?;
        sqlx::query(
            r#"
            INSERT INTO devices(id, ip, hostname, mac, vendor, name_override, selected, ignored, source, last_seen)
            SELECT ?, ip, hostname, mac, vendor, name_override, selected, ignored, source, last_seen
            FROM devices WHERE id = ?
            ON CONFLICT(id) DO NOTHING
            "#,
        )
        .bind(&id)
        .bind(&old_id)
        .execute(pool)
        .await?;
        sqlx::query("DELETE FROM devices WHERE id = ?")
            .bind(&old_id)
            .execute(pool)
            .await?;
    }

    sqlx::query(
        r#"
        INSERT INTO devices(id, ip, hostname, mac, vendor, source, last_seen)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        ON CONFLICT(id) DO UPDATE SET
            ip = excluded.ip,
            hostname = COALESCE(excluded.hostname, devices.hostname),
            mac = COALESCE(excluded.mac, devices.mac),
            vendor = COALESCE(excluded.vendor, devices.vendor),
            source = excluded.source,
            last_seen = excluded.last_seen
        "#,
    )
    .bind(&id)
    .bind(&discovered.ip)
    .bind(&discovered.hostname)
    .bind(&discovered.mac)
    .bind(&discovered.vendor)
    .bind(&discovered.source)
    .bind(now)
    .execute(pool)
    .await?;

    get_device(pool, &id).await
}

async fn migrate_favorite_device_keys(pool: &SqlitePool, old_id: &str, new_id: &str) -> Result<()> {
    let old_prefix = format!("{old_id}:");
    let rows = sqlx::query("SELECT service_key FROM favorites WHERE service_key LIKE ?")
        .bind(format!("{old_prefix}%"))
        .fetch_all(pool)
        .await?;

    for row in rows {
        let old_key: String = row.get("service_key");
        let Some(port_suffix) = old_key.strip_prefix(&old_prefix) else {
            continue;
        };
        let new_key = format!("{new_id}:{port_suffix}");
        sqlx::query(
            r#"
            INSERT INTO favorites(service_key, created_at)
            SELECT ?, created_at FROM favorites WHERE service_key = ?
            ON CONFLICT(service_key) DO NOTHING
            "#,
        )
        .bind(&new_key)
        .bind(&old_key)
        .execute(pool)
        .await?;
        sqlx::query("DELETE FROM favorites WHERE service_key = ?")
            .bind(&old_key)
            .execute(pool)
            .await?;
    }

    Ok(())
}

pub async fn get_device(pool: &SqlitePool, id: &str) -> Result<Device> {
    let row = sqlx::query("SELECT * FROM devices WHERE id = ?")
        .bind(id)
        .fetch_one(pool)
        .await?;
    Ok(device_from_row(row))
}

pub async fn list_devices(pool: &SqlitePool) -> Result<Vec<Device>> {
    let rows =
        sqlx::query("SELECT * FROM devices ORDER BY selected DESC, hostname IS NULL, hostname, ip")
            .fetch_all(pool)
            .await?;
    Ok(rows.into_iter().map(device_from_row).collect())
}

pub async fn delete_unselected_devices_without_services(
    pool: &SqlitePool,
    ids: &[String],
) -> Result<()> {
    for id in ids {
        sqlx::query(
            r#"
            DELETE FROM devices
            WHERE id = ?
              AND selected = 0
              AND NOT EXISTS (
                SELECT 1 FROM services WHERE services.device_id = devices.id
              )
              AND NOT EXISTS (
                SELECT 1 FROM favorites WHERE favorites.service_key LIKE devices.id || ':%'
              )
            "#,
        )
        .bind(id)
        .execute(pool)
        .await?;
    }

    Ok(())
}

pub async fn list_favorite_keys(pool: &SqlitePool) -> Result<Vec<String>> {
    let rows = sqlx::query("SELECT service_key FROM favorites ORDER BY created_at ASC")
        .fetch_all(pool)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("service_key"))
        .collect())
}

pub async fn list_favorite_services(pool: &SqlitePool) -> Result<Vec<Service>> {
    let rows = sqlx::query(
        r#"
        SELECT services.*
        FROM services
        INNER JOIN favorites
            ON favorites.service_key = services.device_id || ':' || CAST(services.port AS TEXT)
        INNER JOIN devices
            ON devices.id = services.device_id
        WHERE devices.selected = 1 AND devices.ignored = 0
        ORDER BY favorites.created_at ASC
        "#,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(service_from_row).collect())
}

pub async fn set_favorite(
    pool: &SqlitePool,
    service_key: &str,
    favorite: bool,
) -> Result<Vec<String>> {
    let key = service_key.trim();
    if favorite && !key.is_empty() {
        sqlx::query(
            r#"
            INSERT INTO favorites(service_key, created_at) VALUES (?, ?)
            ON CONFLICT(service_key) DO NOTHING
            "#,
        )
        .bind(key)
        .bind(Utc::now().to_rfc3339())
        .execute(pool)
        .await?;
    } else {
        sqlx::query("DELETE FROM favorites WHERE service_key = ?")
            .bind(key)
            .execute(pool)
            .await?;
    }

    list_favorite_keys(pool).await
}

pub async fn list_selected_devices(pool: &SqlitePool) -> Result<Vec<Device>> {
    let rows = sqlx::query("SELECT * FROM devices WHERE selected = 1 AND ignored = 0 ORDER BY ip")
        .fetch_all(pool)
        .await?;
    Ok(rows.into_iter().map(device_from_row).collect())
}

pub async fn update_device_flags(
    pool: &SqlitePool,
    id: &str,
    selected: bool,
    ignored: bool,
    name_override: Option<String>,
) -> Result<Device> {
    sqlx::query("UPDATE devices SET selected = ?, ignored = ?, name_override = ? WHERE id = ?")
        .bind(selected)
        .bind(ignored)
        .bind(name_override.and_then(|value| {
            let trimmed = value.trim().to_string();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed)
            }
        }))
        .bind(id)
        .execute(pool)
        .await?;
    get_device(pool, id).await
}

pub async fn upsert_service(pool: &SqlitePool, device: &Device, hit: &ProbeHit) -> Result<Service> {
    upsert_service_for_device(pool, &device.id, &device.ip, hit).await
}

pub async fn upsert_service_for_device(
    pool: &SqlitePool,
    device_id: &str,
    ip: &str,
    hit: &ProbeHit,
) -> Result<Service> {
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        r#"
        INSERT INTO services(
            device_id, ip, port, scheme, url, title, status_code, server,
            first_seen, last_seen, last_checked, active, last_failure
        )
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, NULL)
        ON CONFLICT(device_id, port) DO UPDATE SET
            ip = excluded.ip,
            scheme = excluded.scheme,
            url = excluded.url,
            title = excluded.title,
            status_code = excluded.status_code,
            server = excluded.server,
            last_seen = excluded.last_seen,
            last_checked = excluded.last_checked,
            active = 1,
            last_failure = NULL
        "#,
    )
    .bind(device_id)
    .bind(ip)
    .bind(i64::from(hit.port))
    .bind(&hit.scheme)
    .bind(&hit.url)
    .bind(&hit.title)
    .bind(hit.status_code)
    .bind(&hit.server)
    .bind(&now)
    .bind(&now)
    .bind(&now)
    .execute(pool)
    .await?;

    get_service_by_device_port(pool, device_id, hit.port).await
}

pub async fn mark_service_inactive(
    pool: &SqlitePool,
    device_id: &str,
    port: u16,
    failure: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE services
        SET active = 0, last_checked = ?, last_failure = ?
        WHERE device_id = ? AND port = ?
        "#,
    )
    .bind(now)
    .bind(failure)
    .bind(device_id)
    .bind(i64::from(port))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_device_services_inactive(
    pool: &SqlitePool,
    device_id: &str,
    failure: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE services
        SET active = 0, last_checked = ?, last_failure = ?
        WHERE device_id = ?
        "#,
    )
    .bind(now)
    .bind(failure)
    .bind(device_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_service_active(pool: &SqlitePool, device_id: &str, port: u16) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE services
        SET active = 1, last_seen = ?, last_checked = ?, last_failure = NULL
        WHERE device_id = ? AND port = ?
        "#,
    )
    .bind(&now)
    .bind(&now)
    .bind(device_id)
    .bind(i64::from(port))
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn mark_missing_services_inactive(
    pool: &SqlitePool,
    device_id: &str,
    scan_started_at: &str,
) -> Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        UPDATE services
        SET active = 0, last_checked = ?, last_failure = 'Not seen in latest scan'
        WHERE device_id = ? AND active = 1 AND last_checked < ?
        "#,
    )
    .bind(now)
    .bind(device_id)
    .bind(scan_started_at)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_retained_services(
    pool: &SqlitePool,
    retention_days: i64,
) -> Result<Vec<Service>> {
    let cutoff = (Utc::now() - Duration::days(retention_days)).to_rfc3339();
    let rows = sqlx::query(
        r#"
        SELECT services.*
        FROM services
        INNER JOIN devices
            ON devices.id = services.device_id
        WHERE devices.selected = 1
            AND devices.ignored = 0
            AND (services.active = 1 OR services.last_seen >= ?)
        ORDER BY
            services.active DESC,
            CASE WHEN services.title IS NULL OR TRIM(services.title) = '' THEN 1 ELSE 0 END ASC,
            COALESCE(services.title, services.ip) COLLATE NOCASE ASC,
            services.ip ASC,
            services.port ASC
        "#,
    )
    .bind(cutoff)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(service_from_row).collect())
}

pub async fn insert_scan_history(
    pool: &SqlitePool,
    started_at: &str,
    selected_devices: usize,
    discovered_services: usize,
) -> Result<()> {
    let finished_at = Utc::now().to_rfc3339();
    sqlx::query(
        r#"
        INSERT INTO scan_history(started_at, finished_at, selected_devices, discovered_services)
        VALUES (?, ?, ?, ?)
        "#,
    )
    .bind(started_at)
    .bind(finished_at)
    .bind(selected_devices as i64)
    .bind(discovered_services as i64)
    .execute(pool)
    .await?;
    Ok(())
}

async fn get_service_by_device_port(
    pool: &SqlitePool,
    device_id: &str,
    port: u16,
) -> Result<Service> {
    let row = sqlx::query("SELECT * FROM services WHERE device_id = ? AND port = ?")
        .bind(device_id)
        .bind(i64::from(port))
        .fetch_one(pool)
        .await?;
    Ok(service_from_row(row))
}

fn device_from_row(row: sqlx::sqlite::SqliteRow) -> Device {
    Device {
        id: row.get("id"),
        ip: row.get("ip"),
        hostname: row.get("hostname"),
        mac: row.get("mac"),
        vendor: row.get("vendor"),
        name_override: row.get("name_override"),
        selected: row.get::<i64, _>("selected") != 0,
        ignored: row.get::<i64, _>("ignored") != 0,
        source: row.get("source"),
        last_seen: row.get("last_seen"),
    }
}

fn service_from_row(row: sqlx::sqlite::SqliteRow) -> Service {
    Service {
        id: row.get("id"),
        device_id: row.get("device_id"),
        ip: row.get("ip"),
        port: row.get::<i64, _>("port") as u16,
        scheme: row.get("scheme"),
        url: row.get("url"),
        title: row.get("title"),
        status_code: row.get("status_code"),
        server: row.get("server"),
        first_seen: row.get("first_seen"),
        last_seen: row.get("last_seen"),
        last_checked: row.get("last_checked"),
        active: row.get::<i64, _>("active") != 0,
        last_failure: row.get("last_failure"),
    }
}

fn parse_bool(values: &HashMap<String, String>, key: &str, default: bool) -> bool {
    values
        .get(key)
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or(default)
}

fn parse_u64(values: &HashMap<String, String>, key: &str, default: u64) -> u64 {
    values
        .get(key)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(default)
}

fn parse_i64(values: &HashMap<String, String>, key: &str, default: i64) -> i64 {
    values
        .get(key)
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(default)
}

fn parse_usize(values: &HashMap<String, String>, key: &str, default: usize) -> usize {
    values
        .get(key)
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, Utc};
    use tempfile::tempdir;

    use super::*;
    use crate::models::{DiscoveredDevice, ProbeHit};

    #[tokio::test]
    async fn retention_hides_expired_inactive_services() {
        let dir = tempdir().unwrap();
        let pool = connect(&dir.path().join("test.sqlite3")).await.unwrap();
        let device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.50".to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();
        update_device_flags(&pool, &device.id, true, false, None)
            .await
            .unwrap();
        upsert_service(
            &pool,
            &device,
            &ProbeHit {
                port: 8080,
                scheme: "http".to_string(),
                url: "http://192.168.1.50:8080/".to_string(),
                title: Some("Old".to_string()),
                status_code: Some(200),
                server: None,
            },
        )
        .await
        .unwrap();

        let old = (Utc::now() - Duration::days(31)).to_rfc3339();
        sqlx::query("UPDATE services SET active = 0, last_seen = ?")
            .bind(old)
            .execute(&pool)
            .await
            .unwrap();

        assert!(list_retained_services(&pool, 30).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn retention_keeps_recent_inactive_services() {
        let dir = tempdir().unwrap();
        let pool = connect(&dir.path().join("test.sqlite3")).await.unwrap();
        let device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.51".to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();
        update_device_flags(&pool, &device.id, true, false, None)
            .await
            .unwrap();
        upsert_service(
            &pool,
            &device,
            &ProbeHit {
                port: 3000,
                scheme: "http".to_string(),
                url: "http://192.168.1.51:3000/".to_string(),
                title: Some("Recent".to_string()),
                status_code: Some(200),
                server: None,
            },
        )
        .await
        .unwrap();

        sqlx::query("UPDATE services SET active = 0")
            .execute(&pool)
            .await
            .unwrap();

        assert_eq!(list_retained_services(&pool, 30).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn retained_services_hide_unselected_devices() {
        let dir = tempdir().unwrap();
        let pool = connect(&dir.path().join("test.sqlite3")).await.unwrap();
        let device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.54".to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();
        update_device_flags(&pool, &device.id, true, false, None)
            .await
            .unwrap();
        upsert_service(
            &pool,
            &device,
            &ProbeHit {
                port: 8082,
                scheme: "http".to_string(),
                url: "http://192.168.1.54:8082/".to_string(),
                title: Some("Hidden when off".to_string()),
                status_code: Some(200),
                server: None,
            },
        )
        .await
        .unwrap();

        assert_eq!(list_retained_services(&pool, 30).await.unwrap().len(), 1);

        update_device_flags(&pool, &device.id, false, false, None)
            .await
            .unwrap();

        assert!(list_retained_services(&pool, 30).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn marking_service_active_preserves_metadata() {
        let dir = tempdir().unwrap();
        let pool = connect(&dir.path().join("test.sqlite3")).await.unwrap();
        let device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.53".to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();
        update_device_flags(&pool, &device.id, true, false, None)
            .await
            .unwrap();
        upsert_service(
            &pool,
            &device,
            &ProbeHit {
                port: 8081,
                scheme: "http".to_string(),
                url: "http://192.168.1.53:8081/".to_string(),
                title: Some("Favorite App".to_string()),
                status_code: Some(200),
                server: Some("test-server".to_string()),
            },
        )
        .await
        .unwrap();

        mark_service_inactive(&pool, &device.id, 8081, "temporary miss")
            .await
            .unwrap();
        mark_service_active(&pool, &device.id, 8081).await.unwrap();

        let services = list_retained_services(&pool, 30).await.unwrap();
        assert_eq!(services[0].title.as_deref(), Some("Favorite App"));
        assert_eq!(services[0].server.as_deref(), Some("test-server"));
        assert!(services[0].active);
        assert!(services[0].last_failure.is_none());
    }

    #[tokio::test]
    async fn favorites_persist_and_migrate_when_device_id_changes() {
        let dir = tempdir().unwrap();
        let pool = connect(&dir.path().join("test.sqlite3")).await.unwrap();
        let ip_device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.52".to_string(),
                hostname: None,
                mac: None,
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();

        let favorite_key = format!("{}:8080", ip_device.id);
        assert_eq!(
            set_favorite(&pool, &favorite_key, true).await.unwrap(),
            vec![favorite_key.clone()]
        );

        let mac_device = upsert_device(
            &pool,
            &DiscoveredDevice {
                ip: "192.168.1.52".to_string(),
                hostname: None,
                mac: Some("aa:bb:cc:dd:ee:ff".to_string()),
                vendor: None,
                source: "test".to_string(),
            },
        )
        .await
        .unwrap();

        assert_eq!(
            list_favorite_keys(&pool).await.unwrap(),
            vec![format!("{}:8080", mac_device.id)]
        );
    }
}
