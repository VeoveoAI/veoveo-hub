//! Public verification against records an administrator tampered with in a fresh
//! PostgreSQL database.
#[path = "support/database.rs"]
mod database;
use database::TestDb;

use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::{TimeDelta, Utc};
use secrecy::ExposeSecret;
use serde::Deserialize;
use std::{num::NonZeroU32, process::Output, sync::Arc, time::Duration};
use tokio::process::Command;
use veoveo_audit::{integrity::AuditSigningKey, *};

#[derive(Clone, Copy, Debug)]
enum Attack {
    ChangedRecord,
    DeletedRecord,
    DeletedBlock,
    ForgedSignature,
    BackdatedInsert,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct VerificationReport {
    checkpoint: AuditCheckpoint,
    blocks: u64,
    records: u64,
    clock_findings: u64,
}

fn decode_verification_report(bytes: &[u8]) -> VerificationReport {
    let current: serde_json::Value = serde_json::from_slice(bytes).unwrap();
    let report = serde_json::from_slice(bytes).unwrap();
    for keep_current in [false, true] {
        let mut bad = current.clone();
        let fields = bad.as_object_mut().unwrap();
        let count = fields.get("clockFindings").unwrap().clone();
        if !keep_current {
            fields.remove("clockFindings");
        }
        fields.insert("clock_findings".to_owned(), count);
        assert!(serde_json::from_value::<VerificationReport>(bad).is_err());
    }
    report
}

/// The audit log of a fresh database, read and written as the installation owner.
fn audit(db: &TestDb) -> AuditStore {
    AuditStore::new(db.a.clone(), veoveo_service_gateway::audit_registry())
}

async fn verify(db: &TestDb, key: &AuditSigningKey) -> Output {
    audit_command(
        db,
        &[
            "verify",
            "--installation",
            "--public-key",
            &STANDARD.encode(key.public_key()),
        ],
    )
    .await
}

async fn audit_command(db: &TestDb, args: &[&str]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_veoveo-service-gateway"));
    // A fixture must neither load the worktree .env nor inherit installation credentials.
    command
        .env_clear()
        .current_dir("/")
        .kill_on_drop(true)
        .arg("audit")
        .args(args)
        // Match the hosted environment without borrowing installation services.
        // CLI commands must not initialize server exporters or put logs on stdout.
        .env("OTEL_EXPORTER_OTLP_ENDPOINT", "http://127.0.0.1:9")
        .env("RUST_LOG", "info")
        .env("VEOVEO_DATABASE_URL", db.url())
        .env("VEOVEO_DATABASE_PASSWORD", db.password().expose_secret());
    if let Some(path) = std::env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", path);
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("audit command exceeded 30 seconds")
        .expect("cannot execute gateway");
    assert!(
        output.stdout.len() <= 65_536 && output.stderr.len() <= 65_536,
        "audit CLI fixture output exceeded 64 KiB"
    );
    output
}

fn diagnostic(output: &Output, db: &TestDb) -> String {
    String::from_utf8_lossy(&output.stderr).replace(db.password().expose_secret(), "[REDACTED]")
}

async fn qualify_export(db: &TestDb) {
    let service = AuditService::start(
        audit(db),
        Arc::new(AuditSigningKey::from_seed(&[38; 32])),
        NonZeroU32::new(7).unwrap(),
        Default::default(),
    )
    .unwrap();
    let output = audit_command(db, &["export", "--installation"]).await;
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    assert!(
        output.status.success(),
        "audit export failed: {}",
        diagnostic(&output, db)
    );
    let lines: Vec<AuditExportLine> = String::from_utf8(output.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            veoveo_service_gateway::audit_registry()
                .decoder()
                .from_str(line)
                .expect("every stdout line must be audit JSONL")
        })
        .collect();
    let Some(AuditExportLine::Header {
        checkpoint: first, ..
    }) = lines.first()
    else {
        panic!("export must begin with its header")
    };
    let Some(AuditExportLine::Complete {
        records,
        checkpoint: last,
    }) = lines.last()
    else {
        panic!("export must end with its completion footer")
    };
    assert_eq!(first, last);
    assert!(*records > 0, "export includes its committed access record");
    assert_eq!(*records as usize, lines.len() - 2);
    assert!(
        lines[1..lines.len() - 1]
            .iter()
            .all(|line| matches!(line, AuditExportLine::Record { .. }))
    );
}

async fn qualify(db: &TestDb, attack: Attack) {
    let store = audit(db);
    let key = Arc::new(AuditSigningKey::from_seed(&[37; 32]));
    let service = AuditService::start(
        store.clone(),
        key.clone(),
        NonZeroU32::new(7).unwrap(),
        Default::default(),
    )
    .unwrap();
    let scope = AuditReadScope::new(None, true);
    let partition = AuditPartition::Installation;
    let mut ids = Vec::new();
    let audit = veoveo_service_gateway::audit::gateway_audit();
    for detail in [
        audit
            .detail(&veoveo_sdk_rust::audit::Read {
                method: veoveo_sdk_rust::audit::McpReadMethod::ResourceRead,
            })
            .unwrap(),
        audit
            .detail(&veoveo_service_gateway::audit::TaskStatusRead {})
            .unwrap(),
    ] {
        let mut builder = AuditDraft::builder(
            AuditRequest::background(),
            AuditTarget::Installation,
            detail,
            AuditOutcome::Allowed,
            AuditReason::Accepted,
        );
        if matches!(attack, Attack::BackdatedInsert) && !ids.is_empty() {
            builder = builder.occurred_at(Utc::now() - TimeDelta::days(1));
        }
        let draft = builder.build().unwrap();
        ids.push(draft.id());
        store.append(&[draft]).await.unwrap();
        tokio::time::timeout(
            Duration::from_secs(20),
            store.wait_sealed(&scope, &partition, *ids.last().unwrap()),
        )
        .await
        .expect("fixture record was not sealed")
        .unwrap();
    }
    service.shutdown(Duration::from_secs(15)).await.unwrap();
    let blocks = store.blocks(&scope, &partition, None, 10).await.unwrap();
    assert_eq!(blocks.len(), 2);

    if !matches!(attack, Attack::BackdatedInsert) {
        let output = verify(db, &key).await;
        assert!(
            output.status.success(),
            "pristine audit CLI verification failed: {}",
            diagnostic(&output, db)
        );
        let report: VerificationReport = decode_verification_report(&output.stdout);
        assert_eq!(
            (report.blocks, report.records, report.clock_findings),
            (2, 2, 0)
        );
        assert_eq!(report.checkpoint, blocks[1].checkpoint());
    }

    // The installation owner owns the audit tables and models a database administrator.
    let pool = db.a.pool();
    let record = ids[0].as_uuid();
    let block_partition = partition.storage_key();
    let block_sequence = i64::try_from(blocks[0].head.sequence.get()).unwrap();
    match attack {
        Attack::ChangedRecord => {
            sqlx::query_file!("tests/audit_cli/allow_record_updates.sql")
                .execute(pool)
                .await
                .unwrap();
            sqlx::query_file!("tests/audit_cli/change_record.sql", record)
                .execute(pool)
                .await
                .unwrap();
        }
        Attack::DeletedRecord => {
            sqlx::query_file!("tests/audit_cli/delete_record.sql", record)
                .execute(pool)
                .await
                .unwrap();
        }
        Attack::DeletedBlock => {
            sqlx::query_file!(
                "tests/audit_cli/delete_block.sql",
                block_partition,
                block_sequence
            )
            .execute(pool)
            .await
            .unwrap();
        }
        Attack::ForgedSignature => {
            sqlx::query_file!("tests/audit_cli/allow_block_updates.sql")
                .execute(pool)
                .await
                .unwrap();
            sqlx::query_file!(
                "tests/audit_cli/forge_signature.sql",
                block_partition,
                block_sequence,
                serde_json::to_value(AuditSignature::from_bytes([0; 64])).unwrap(),
            )
            .execute(pool)
            .await
            .unwrap();
        }
        Attack::BackdatedInsert => {}
    }
    let output = verify(db, &key).await;
    assert!(
        !output.status.success(),
        "{attack:?} was accepted by audit verify"
    );
    let error = diagnostic(&output, db);
    let expected = match attack {
        Attack::ChangedRecord => "record hash does not match",
        Attack::DeletedRecord => "audit row identity or document is inconsistent",
        Attack::DeletedBlock => "missing or mismatched link",
        Attack::ForgedSignature => "signature is invalid",
        Attack::BackdatedInsert => "BackdatedRecord",
    };
    assert!(
        error.contains(expected),
        "{attack:?}: expected {expected:?}, received {error}"
    );
    if matches!(attack, Attack::BackdatedInsert) {
        let report: VerificationReport = decode_verification_report(&output.stdout);
        assert_eq!(
            (report.blocks, report.records, report.clock_findings),
            (2, 2, 1)
        );
    }
    println!("{{\"audit_cli\":\"{attack:?}\",\"detected\":true}}");
}

#[tokio::test]
async fn public_verify_detects_root_tampering_and_backdated_inserts() {
    tokio::time::timeout(Duration::from_secs(240), async {
        for attack in [
            Attack::ChangedRecord,
            Attack::DeletedRecord,
            Attack::DeletedBlock,
            Attack::ForgedSignature,
            Attack::BackdatedInsert,
        ] {
            qualify(&TestDb::new().await, attack).await;
        }
        qualify_export(&TestDb::new().await).await;
    })
    .await
    .expect("audit CLI acceptance exceeded 240 seconds");
}

#[tokio::test]
async fn keygen_summary_is_current_while_seed_and_public_key_bytes_agree() {
    use std::os::unix::fs::PermissionsExt;
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct PublicKeySummary {
        key_id: veoveo_types::Sha256Digest,
        public_key: String,
    }
    let directory = tempfile::tempdir().unwrap();
    let output_path = directory.path().join("seed");
    let mut command = Command::new(env!("CARGO_BIN_EXE_veoveo-service-gateway"));
    command
        .env_clear()
        .current_dir("/")
        .kill_on_drop(true)
        .args(["audit", "keygen", "--secret-out"])
        .arg(&output_path);
    if let Some(path) = std::env::var_os("LD_LIBRARY_PATH") {
        command.env("LD_LIBRARY_PATH", path);
    }
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .expect("keygen exceeded 30 seconds")
        .unwrap();
    assert!(
        output.status.success(),
        "keygen failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let current: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary: PublicKeySummary = serde_json::from_slice(&output.stdout).unwrap();
    let encoded = zeroize::Zeroizing::new(std::fs::read(&output_path).unwrap());
    let decoded = zeroize::Zeroizing::new(STANDARD.decode(encoded.as_slice()).unwrap());
    assert_eq!(decoded.len(), 32, "keygen seed length changed");
    let mut seed = zeroize::Zeroizing::new([0u8; 32]);
    seed.copy_from_slice(decoded.as_slice());
    let key = AuditSigningKey::from_seed(&seed);
    assert_eq!(summary.key_id, key.key_id());
    assert_eq!(
        STANDARD.decode(summary.public_key).unwrap(),
        key.public_key()
    );
    assert_eq!(
        std::fs::metadata(&output_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(
        !output
            .stdout
            .windows(encoded.len())
            .any(|bytes| bytes == encoded.as_slice())
    );
    for (name, retired) in [("keyId", "key_id"), ("publicKey", "public_key")] {
        for keep_current in [false, true] {
            let mut bad = current.clone();
            let fields = bad.as_object_mut().unwrap();
            let value = fields.get(name).unwrap().clone();
            if !keep_current {
                fields.remove(name);
            }
            fields.insert(retired.into(), value);
            assert!(serde_json::from_value::<PublicKeySummary>(bad.clone()).is_err());
            assert!(
                serde_json::from_slice::<PublicKeySummary>(&serde_json::to_vec(&bad).unwrap())
                    .is_err()
            );
        }
    }
}
