use super::*;
use std::collections::BTreeMap;

const ID: &str = "123e4567-e89b-42d3-a456-426614174000";

fn folder() -> std::path::PathBuf {
    // Explicitly retain test folders: this project forbids recursive deletion.
    tempfile::tempdir().unwrap().keep()
}

#[test]
fn folder_create_read_and_compare_swap_never_overwrite_unknown_changes() {
    let root = folder();
    let provider = LocalFolderProvider::new(&root).unwrap();
    provider.test_connection().unwrap();
    let first = provider.write(ID, "first", None).unwrap();
    assert_eq!(provider.list().unwrap(), [ID]);
    assert_eq!(provider.read(ID).unwrap().unwrap().content, "first");
    assert!(provider.write(ID, "collision", None).is_err());
    let second = provider
        .write(ID, "second", first.revision.as_deref())
        .unwrap();
    assert!(
        provider
            .write(ID, "stale", first.revision.as_deref())
            .is_err()
    );
    assert_eq!(provider.read(ID).unwrap().unwrap().hash, second.hash);
    std::fs::write(
        root.join(format!("StudyApp/notes/{ID}.md")),
        "external edit",
    )
    .unwrap();
    assert!(matches!(
        provider.write(ID, "stale after external edit", second.revision.as_deref()),
        Err(SyncError::Conflict)
    ));
    assert_eq!(provider.read(ID).unwrap().unwrap().content, "external edit");
    assert!(provider.read("../../outside").is_err());
}

#[test]
fn planner_conflict_and_external_copy_preserve_source_and_baseline() {
    let provider = LocalFolderProvider::new(folder()).unwrap();
    provider.write(ID, "remote", None).unwrap();
    let mut notes = vec![SyncNote {
        id: ID.into(),
        title: "题目".into(),
        content: "local".into(),
        hash: content_hash("local"),
        managed: true,
    }];
    let mut baselines = BTreeMap::new();
    baselines.insert(
        ID.into(),
        Baseline {
            local_hash: content_hash("old"),
            remote_hash: content_hash("old"),
        },
    );
    let plan = plan_sync(&provider, &notes, &baselines).unwrap();
    assert!(
        matches!(&plan[0].action, SyncAction::Conflict {local_content, remote_content} if local_content == "local" && remote_content == "remote")
    );
    assert_eq!(baselines[ID].local_hash, content_hash("old"));
    notes[0].managed = false;
    let plan = plan_sync(&provider, &notes, &baselines).unwrap();
    assert!(matches!(&plan[0].action, SyncAction::CopyExternal {content} if content == "remote"));
    assert_eq!(plan[0].next_baseline.local_hash, content_hash("local"));
    assert_eq!(plan[0].next_baseline.remote_hash, content_hash("remote"));
}

#[test]
fn independent_folders_roundtrip_and_repeat_is_unchanged() {
    let a = LocalFolderProvider::new(folder()).unwrap();
    let b = LocalFolderProvider::new(folder()).unwrap();
    a.write(ID, "hello", None).unwrap();
    let local = a.read(ID).unwrap().unwrap();
    let notes = [SyncNote {
        id: ID.into(),
        title: "Hello".into(),
        content: local.content,
        hash: local.hash,
        managed: true,
    }];
    let plan = plan_sync(&b, &notes, &BTreeMap::new()).unwrap();
    match &plan[0].action {
        SyncAction::Push {
            content,
            expected_revision,
        } => {
            b.write(ID, content, expected_revision.as_deref()).unwrap();
        }
        other => panic!("unexpected {other:?}"),
    }
    let baseline = BTreeMap::from([(ID.into(), plan[0].next_baseline.clone())]);
    assert!(matches!(
        plan_sync(&b, &notes, &baseline).unwrap()[0].action,
        SyncAction::Unchanged
    ));
    let incoming = plan_sync(&b, &[], &BTreeMap::new()).unwrap();
    assert!(
        matches!(&incoming[0].action, SyncAction::ImportRemote {content} if content == "hello")
    );
}

type Reply = (u16, Vec<(&'static str, String)>, String);
fn fixture(replies: Vec<Reply>) -> (String, std::thread::JoinHandle<Vec<String>>) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}/dav/", listener.local_addr().unwrap());
    let thread = std::thread::spawn(move || {
        let mut requests = Vec::new();
        for (status, headers, body) in replies {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        assert!(
                            std::time::Instant::now() < deadline,
                            "fixture request timed out"
                        );
                        std::thread::sleep(std::time::Duration::from_millis(5));
                    }
                    Err(e) => panic!("fixture accept: {e}"),
                }
            };
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(5)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut byte = [0];
            while !bytes.ends_with(b"\r\n\r\n") {
                stream.read_exact(&mut byte).unwrap();
                bytes.push(byte[0]);
                assert!(bytes.len() < 64 * 1024);
            }
            let header = String::from_utf8(bytes).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().parse::<usize>().unwrap())
                })
                .unwrap_or(0);
            let mut request_body = vec![0; length];
            stream.read_exact(&mut request_body).unwrap();
            requests.push(format!(
                "{header}{}",
                String::from_utf8(request_body).unwrap()
            ));
            let mut response = format!(
                "HTTP/1.1 {status} Result\r\nContent-Length: {}\r\nConnection: close\r\n",
                body.len()
            );
            for (name, value) in headers {
                response.push_str(&format!("{name}: {value}\r\n"));
            }
            response.push_str("\r\n");
            response.push_str(&body);
            stream.write_all(response.as_bytes()).unwrap();
        }
        requests
    });
    (url, thread)
}

#[test]
fn webdav_auth_listing_get_put_and_precondition_failure() {
    let xml = format!(
        "<d:multistatus xmlns:d=\"DAV:\"><d:response><d:href>/dav/StudyApp/notes/{ID}.md</d:href></d:response><d:response><d:href>https://evil.invalid/dav/StudyApp/notes/{ID}.md</d:href></d:response><d:response><d:href>/other/{ID}.md</d:href></d:response></d:multistatus>"
    );
    let (url, server) = fixture(vec![
        (201, vec![], String::new()),
        (405, vec![], String::new()),
        (207, vec![], xml),
        (200, vec![("ETag", "\"v1\"".into())], "old".into()),
        (204, vec![("ETag", "\"v2\"".into())], String::new()),
        (412, vec![], String::new()),
    ]);
    let provider = WebDavProvider::for_loopback_test(&url, "alice", "secret").unwrap();
    assert_eq!(provider.list().unwrap(), [ID]);
    let old = provider.read(ID).unwrap().unwrap();
    assert_eq!(old.revision.as_deref(), Some("\"v1\""));
    provider.write(ID, "new", old.revision.as_deref()).unwrap();
    assert!(matches!(
        provider.write(ID, "lost", None),
        Err(SyncError::Conflict)
    ));
    let requests = server.join().unwrap();
    assert!(requests[0].starts_with("MKCOL /dav/StudyApp/ "));
    assert!(requests[1].starts_with("MKCOL /dav/StudyApp/notes/ "));
    assert!(requests.iter().all(|r| {
        r.to_lowercase()
            .contains("authorization: basic ywxpy2u6c2vjcmv0\r\n")
    }));
    assert!(requests[2].to_lowercase().contains("depth: 1\r\n"));
    assert!(requests[4].to_lowercase().contains("if-match: \"v1\"\r\n"));
    assert!(requests[5].to_lowercase().contains("if-none-match: *\r\n"));
}

#[test]
fn webdav_redirect_auth_errors_and_insecure_configuration_are_rejected() {
    assert!(WebDavProvider::new("http://example.com", "", "").is_err());
    assert!(WebDavProvider::new("https://alice:secret@example.com", "", "").is_err());
    assert!(WebDavProvider::for_loopback_test("http://example.com", "", "").is_err());
    for status in [301, 401, 403, 500] {
        let (url, server) = fixture(vec![(
            status,
            vec![("Location", "https://evil.invalid/".into())],
            String::new(),
        )]);
        let provider = WebDavProvider::for_loopback_test(&url, "alice", "secret").unwrap();
        let error = provider.read(ID).unwrap_err().to_string();
        assert!(!error.contains("secret"));
        assert!(!error.contains("alice"));
        assert_eq!(server.join().unwrap().len(), 1);
    }
}

#[test]
fn missing_etag_cannot_authorize_remote_overwrite() {
    let (url, server) = fixture(vec![
        (201, vec![], String::new()),
        (201, vec![], String::new()),
        (
            207,
            vec![],
            format!(
                "<multistatus xmlns=\"DAV:\"><response><href>/dav/StudyApp/notes/{ID}.md</href></response></multistatus>"
            ),
        ),
        (200, vec![], "old".into()),
    ]);
    let provider = WebDavProvider::for_loopback_test(&url, "", "").unwrap();
    let notes = [SyncNote {
        id: ID.into(),
        title: "t".into(),
        content: "new".into(),
        hash: content_hash("new"),
        managed: true,
    }];
    let baseline = BTreeMap::from([(
        ID.into(),
        Baseline {
            local_hash: content_hash("old"),
            remote_hash: content_hash("old"),
        },
    )]);
    assert!(matches!(
        plan_sync(&provider, &notes, &baseline),
        Err(SyncError::MissingRevision)
    ));
    server.join().unwrap();
}

#[test]
fn planner_one_sided_changes_and_missing_remote_are_non_destructive() {
    let provider = LocalFolderProvider::new(folder()).unwrap();
    let original = provider.write(ID, "old", None).unwrap();
    let mut notes = [SyncNote {
        id: ID.into(),
        title: "title".into(),
        content: "old".into(),
        hash: content_hash("old"),
        managed: true,
    }];
    let baseline = BTreeMap::from([(
        ID.into(),
        Baseline {
            local_hash: content_hash("old"),
            remote_hash: content_hash("old"),
        },
    )]);
    provider
        .write(ID, "remote change", original.revision.as_deref())
        .unwrap();
    let incoming = plan_sync(&provider, &notes, &baseline).unwrap();
    assert!(
        matches!(&incoming[0].action, SyncAction::Pull {content} if content == "remote change")
    );
    assert_eq!(
        incoming[0].expected_local_hash.as_deref(),
        Some(content_hash("old").as_str())
    );
    assert_eq!(
        incoming[0].next_baseline.local_hash,
        content_hash("remote change")
    );
    notes[0].content = "local change".into();
    notes[0].hash = content_hash("local change");
    let baseline = BTreeMap::from([(
        ID.into(),
        Baseline {
            local_hash: content_hash("old"),
            remote_hash: content_hash("remote change"),
        },
    )]);
    let outgoing = plan_sync(&provider, &notes, &baseline).unwrap();
    assert!(
        matches!(&outgoing[0].action, SyncAction::Push {content, expected_revision: Some(_)} if content == "local change")
    );
    assert_eq!(outgoing[0].next_baseline.remote_hash, notes[0].hash);
    let empty = LocalFolderProvider::new(folder()).unwrap();
    assert!(matches!(
        plan_sync(&empty, &notes, &baseline).unwrap()[0].action,
        SyncAction::Push {
            expected_revision: None,
            ..
        }
    ));
    notes[0].hash = "incorrect".into();
    assert!(matches!(
        plan_sync(&provider, &notes, &baseline),
        Err(SyncError::InvalidContent)
    ));
}

#[test]
fn folder_lock_busy_invalid_content_and_unknown_files_are_safe() {
    let root = folder();
    let provider = LocalFolderProvider::new(&root).unwrap();
    std::fs::write(root.join("StudyApp/notes/readme.md"), "unrelated").unwrap();
    std::fs::write(root.join("StudyApp/notes/.pending"), "unfinished").unwrap();
    assert!(provider.list().unwrap().is_empty());
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("StudyApp/.sync.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(matches!(
        provider.write(ID, "blocked", None),
        Err(SyncError::LockBusy)
    ));
    drop(lock);
    assert!(matches!(
        provider.write(ID, "bad\0text", None),
        Err(SyncError::InvalidContent)
    ));
    assert!(matches!(
        provider.write(ID, &"a".repeat(MAX_CONTENT_BYTES + 1), None),
        Err(SyncError::LimitExceeded)
    ));
    std::fs::write(root.join(format!("StudyApp/notes/{ID}.md")), [255, 254]).unwrap();
    assert!(matches!(provider.read(ID), Err(SyncError::InvalidContent)));
    assert_eq!(
        std::fs::read_to_string(root.join("StudyApp/notes/readme.md")).unwrap(),
        "unrelated"
    );
}

#[test]
fn webdav_empty_and_adversarial_listings_are_bounded() {
    for (xml, expected) in [
        ("<d:multistatus xmlns:d=\"DAV:\"/>".to_owned(), Ok(0)),
        (
            "<!DOCTYPE multistatus [<!ENTITY x SYSTEM \"file:///secret\">]><multistatus/>"
                .to_owned(),
            Err(()),
        ),
        ("<multistatus><response>".to_owned(), Err(())),
        (
            format!(
                "<multistatus>{}</multistatus>",
                "<response/>".repeat(MAX_ENTRIES + 1)
            ),
            Err(()),
        ),
        ("x".repeat(MAX_XML_BYTES + 1), Err(())),
        (
            format!(
                "<multistatus><response><href>/dav/StudyApp/notes/%2e%2e/notes/{ID}.md</href></response><response><href>/dav/StudyApp/notes/{ID}.md/extra</href></response><response><href>/dav/StudyApp/notes/not-a-uuid.md</href></response></multistatus>"
            ),
            Ok(0),
        ),
    ] {
        let (url, server) = fixture(vec![
            (201, vec![], String::new()),
            (201, vec![], String::new()),
            (207, vec![], xml),
        ]);
        let provider = WebDavProvider::for_loopback_test(&url, "", "").unwrap();
        let actual = provider.list().map(|ids| ids.len()).map_err(|_| ());
        assert_eq!(actual, expected);
        server.join().unwrap();
    }
}

#[test]
fn webdav_read_limits_and_weak_etag_are_safe() {
    let (url, server) = fixture(vec![
        (200, vec![("ETag", "W/\"weak\"".into())], "text".into()),
        (404, vec![], String::new()),
        (200, vec![], "a".repeat(MAX_CONTENT_BYTES + 1)),
    ]);
    let provider = WebDavProvider::for_loopback_test(&url, "", "").unwrap();
    assert!(provider.read(ID).unwrap().unwrap().revision.is_none());
    assert!(provider.read(ID).unwrap().is_none());
    assert!(matches!(provider.read(ID), Err(SyncError::LimitExceeded)));
    server.join().unwrap();
}

#[cfg(windows)]
#[test]
fn folder_rejects_windows_junction_escape() {
    let root = folder();
    let outside = folder();
    let junction = root.join("StudyApp");
    let output = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(&outside)
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(matches!(
        LocalFolderProvider::new(&root),
        Err(SyncError::InvalidPath)
    ));
    assert!(std::fs::read_dir(&outside).unwrap().next().is_none());
}

#[test]
fn folder_replacement_retains_the_previous_remote_file() {
    let root = folder();
    let provider = LocalFolderProvider::new(&root).unwrap();
    let first = provider.write(ID, "original remote", None).unwrap();
    provider
        .write(ID, "updated remote", first.revision.as_deref())
        .unwrap();
    let retained = root.join("StudyApp/notes/.retained");
    assert!(
        retained.is_dir(),
        "remote originals must remain recoverable"
    );
    assert!(std::fs::read_dir(retained).unwrap().flatten().any(|entry| {
        entry.file_name().to_string_lossy().ends_with(".original")
            && std::fs::read_to_string(entry.path()).unwrap() == "original remote"
    }));
    assert_eq!(provider.list().unwrap(), [ID]);
}

#[test]
fn folder_replace_rechecks_the_object_moved_after_a_path_race() {
    let root = folder();
    let provider = LocalFolderProvider::new(&root).unwrap();
    let first = provider.write(ID, "original", None).unwrap();
    local::BEFORE_RETAIN.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|target| {
            std::fs::rename(target, target.with_extension("retained-by-external-editor")).unwrap();
            std::fs::write(target, "unknown external replacement").unwrap();
        }))
    });
    assert!(matches!(
        provider.write(ID, "incoming", first.revision.as_deref()),
        Err(SyncError::Conflict)
    ));
    assert_eq!(
        provider.read(ID).unwrap().unwrap().content,
        "unknown external replacement"
    );
    assert_eq!(
        std::fs::read_to_string(
            root.join(format!("StudyApp/notes/{ID}.retained-by-external-editor"))
        )
        .unwrap(),
        "original"
    );
}

#[test]
fn folder_replace_never_overwrites_a_file_created_during_publication() {
    let root = folder();
    let provider = LocalFolderProvider::new(&root).unwrap();
    let first = provider.write(ID, "original", None).unwrap();
    local::BEFORE_PUBLISH.with(|hook| {
        *hook.borrow_mut() = Some(Box::new(|target| {
            std::fs::write(target, "new unknown target").unwrap();
        }))
    });
    assert!(matches!(
        provider.write(ID, "incoming", first.revision.as_deref()),
        Err(SyncError::Conflict)
    ));
    assert_eq!(
        provider.read(ID).unwrap().unwrap().content,
        "new unknown target"
    );
    let retained: Vec<_> = std::fs::read_dir(root.join("StudyApp/notes/.retained"))
        .unwrap()
        .flatten()
        .map(|entry| std::fs::read_to_string(entry.path()).unwrap())
        .collect();
    assert!(retained.iter().any(|text| text == "original"));
    assert!(retained.iter().any(|text| text == "incoming"));
}
