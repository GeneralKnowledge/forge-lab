/// A `forge ui explore` walk, as a device bundle (docs/forge-explore.md).
pub struct ExploreRun {
    /// `build` from [`provenance`], read before the walk, plus `installed_as`.
    pub build: Value,
    /// `{ "udid", "ios_major" }`.
    pub device: Value,
    /// The walk's summary: seed, stopped, actions, screens, issues.
    pub explore: Value,
    /// The app's log and crash reports during the walk (`watch.rs`).
    pub watch: Option<Value>,
    /// Files the watch wrote (`device.log`, `crashes/…`).
    pub artifacts: Vec<(&'static str, String)>,
    /// No error-severity issue (a crash, an unlabelled control).
    pub ok: bool,
    pub duration_ms: u64,
    /// `device`, or `fixture` for a walk on a model of the app (no phone):
    /// it says how Forge explored, never how the app behaves.
    pub kind: &'static str,
}

/// Writes `evidence.json` (kind `device`, command `ui explore`) into `dir`,
/// the walk's output folder. A walk proves no requirement (nobody wrote down
/// what it checks), so `requirements` is empty; its issues and their replay
/// files are what it contributes. Lists `explore.json`, every screen's
/// screenshot and tree and every replay file as artifacts.
pub fn write_explore_bundle(run: &ExploreRun, dir: &Path, id: &str) -> Result<PathBuf, String> {
    let mut artifacts = Vec::new();
    if dir.join("explore.json").is_file() {
        artifacts.push(json!({"kind": "explore_report", "path": "explore.json"}));
    }
    for name in ["focus.json", "compare.json"] {
        if dir.join(name).is_file() {
            let kind = name.trim_end_matches(".json");
            artifacts.push(json!({"kind": kind, "path": name}));
        }
    }
    for (sub, ext, kind) in [
        ("screens", "png", "screenshot"),
        ("screens", "json", "ui_tree"),
        ("repro", "toml", "repro"),
        ("visual", "png", "diff"),
    ] {
        let mut files: Vec<String> = fs::read_dir(dir.join(sub))
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e == ext))
            .filter_map(|p| {
                p.file_name()
                    .map(|n| format!("{sub}/{}", n.to_string_lossy()))
            })
            .collect();
        files.sort();
        artifacts.extend(files.into_iter().map(|p| json!({"kind": kind, "path": p})));
    }
    artifacts.extend(
        run.artifacts
            .iter()
            .map(|(kind, path)| json!({"kind": kind, "path": path})),
    );
    let mut doc = json!({
        "schema": SCHEMA,
        "id": id,
        "command": "ui explore",
        "kind": run.kind,
        "ok": run.ok,
        "duration_ms": run.duration_ms,
        "where": {
            "host": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
            "device": run.device,
        },
        "build": run.build,
        "explore": run.explore,
        "watch": run.watch,
        "requirements": [],
        "artifacts": artifacts,
    });
    fs::create_dir_all(dir).map_err(|e| format!("create {}: {e}", dir.display()))?;
    hash_artifacts(&mut doc, dir);
    write_json(dir, &doc)?;
    Ok(dir.join("evidence.json"))
}

    #[test]
    fn an_explore_bundle_lists_its_files_and_proves_no_requirement() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();
        fs::create_dir_all(dir.join("screens")).unwrap();
        fs::create_dir_all(dir.join("repro")).unwrap();
        fs::write(dir.join("explore.json"), "{}").unwrap();
        fs::write(dir.join("screens/ab12.png"), [1u8, 2]).unwrap();
        fs::write(dir.join("screens/ab12.json"), "{}").unwrap();
        fs::write(dir.join("repro/issue-1-app_closed.toml"), "app = \"x\"").unwrap();
        let run = ExploreRun {
            build: json!({"commit": "abc", "dirty": false}),
            device: json!({"udid": "0000", "ios_major": 26}),
            explore: json!({"seed": 1, "issues": [{"issue": "app_closed", "severity": "error"}]}),
            watch: Some(json!({"log_lines": 0, "crashes": []})),
            artifacts: vec![("device_log", "device.log".into())],
            ok: false,
            duration_ms: 5,
            kind: "device",
        };
        let path = write_explore_bundle(&run, dir, "2026-11-08T13-00-00").unwrap();
        let doc: Value = serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(doc["schema"], SCHEMA);
        assert_eq!(doc["kind"], "device");
        assert_eq!(doc["command"], "ui explore");
        assert_eq!(doc["ok"], false);
        assert_eq!(doc["requirements"], json!([]));
        let kinds: Vec<(&str, &str)> = doc["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| (a["kind"].as_str().unwrap(), a["path"].as_str().unwrap()))
            .collect();
        assert_eq!(
            kinds,
            vec![
                ("explore_report", "explore.json"),
                ("screenshot", "screens/ab12.png"),
                ("ui_tree", "screens/ab12.json"),
                ("repro", "repro/issue-1-app_closed.toml"),
                ("device_log", "device.log"),
            ]
        );
        assert_eq!(doc["artifacts"][1]["bytes"], 2);
    }

