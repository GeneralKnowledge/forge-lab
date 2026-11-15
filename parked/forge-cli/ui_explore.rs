/// `forge ui explore`'s options, as given on the command line.
#[derive(Clone)]
pub struct ExploreOptions {
    /// M6.2: aim at what changed since this commit.
    pub since: Option<String>,
    /// M6.2: aim at these files instead of git's list.
    pub changed: Vec<String>,
    /// M6.2: also aim at these identifiers (`a.b`, or `a.*`).
    pub focus: Vec<String>,
    /// The earlier walk to learn from; default the latest when aimed.
    pub previous: Option<PathBuf>,
    pub no_previous: bool,
    /// Issues to replay after the walk.
    pub verify: usize,
    /// Walk a model of the app instead of the phone.
    pub fixture: Option<PathBuf>,
    pub actions: usize,
    pub minutes: f64,
    pub seed: Option<u64>,
    /// `IDENTIFIER=TEXT`
    pub texts: Vec<String>,
    pub skip: Vec<String>,
    pub allow_destructive: bool,
    /// Explore the app on the person's own data, looking only.
    pub real_data: bool,
    pub largest_text: bool,
    /// `dark` or `light`; `None` follows the phone.
    pub appearance: Option<String>,
    /// The app's own launch arguments, after Forge's (for example one that
    /// starts past onboarding with fixture data).
    pub launch_args: Vec<String>,
    pub per_screen: usize,
    pub screenshots: bool,
}

/// The explorer's settings for the app in `project` (its forge.toml bundle
/// id), before the phone says what it is installed as.
pub fn explore_config(project: &Path, options: &ExploreOptions) -> Result<explore::Config, String> {
    let forge_toml = project.join("forge.toml");
    let text = std::fs::read_to_string(&forge_toml).map_err(|e| {
        format!(
            "{}: {e} (forge ui explore works on Forge apps: pass --project <app>)",
            forge_toml.display()
        )
    })?;
    let forge = crate::config::ForgeConfig::parse(&text)?;
    let bundle_id = forge.app.bundle_id;
    let (test_data_args, real_data_safe) = forge
        .ui_test
        .map(|u| (u.test_data_args, u.real_data_safe))
        .unwrap_or_default();
    // Never change the person's data: either the app starts on throwaway
    // data, or the walk only looks (seen on Cati 2026-11-08: a walk on a
    // real library rated a book and saved entries).
    let test_data = !options.real_data && !test_data_args.is_empty();
    if !test_data && !options.real_data {
        return Err(format!(
            "{} has no test-data mode, so exploring would change the data on the phone. \
             Add `[ui_test] test_data_args = [\"-ForgeUITestData\"]` to its forge.toml once the app \
             starts on throwaway data with that argument (see docs/forge-explore.md), or pass \
             --real-data to only look around without changing anything",
            forge_toml.display()
        ));
    }
    if !test_data && options.allow_destructive {
        return Err(
            "--allow-destructive needs the app's test-data mode; it is never used with --real-data"
                .into(),
        );
    }
    let mut fixtures = std::collections::HashMap::new();
    for pair in &options.texts {
        let (id, text) = pair
            .split_once('=')
            .ok_or_else(|| format!("--text {pair:?}: write IDENTIFIER=TEXT"))?;
        fixtures.insert(id.to_owned(), text.to_owned());
    }
    if options.actions < 2 {
        return Err(
            "--actions must be at least 2: the launch is one, and the walk needs another to try anything"
                .into(),
        );
    }
    if !(options.minutes > 0.0 && options.minutes.is_finite()) {
        return Err("--minutes must be more than 0".into());
    }
    let seed = options.seed.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(1)
    });
    Ok(explore::Config {
        app: bundle_id.clone(),
        script_app: bundle_id,
        max_actions: options.actions,
        budget: Duration::from_secs_f64(options.minutes * 60.0),
        seed,
        launch_args: {
            let mut args = if test_data {
                test_data_args
            } else {
                Vec::new()
            };
            if let Some(mode) = &options.appearance {
                args.extend([explore::APPEARANCE_ARG.to_string(), mode.clone()]);
            }
            if options.largest_text {
                args.extend(explore::LARGEST_TEXT_ARGS.iter().map(|a| a.to_string()));
            }
            args.extend(options.launch_args.iter().cloned());
            args
        },
        test_data,
        real_data_safe,
        fixtures,
        allow_destructive: options.allow_destructive,
        skip: options.skip.clone(),
        per_screen: options.per_screen.max(1),
        screenshots: options.screenshots,
        ..explore::Config::default()
    })
}

/// The walk's focus (M6.2): the change's identifiers from `forge ui focus`,
/// identifiers given with `--focus`, and routes and issues from the
/// previous walk. Writes `focus.json` into `out_dir`. Returns the focus and
/// the previous walk's report with its path, when one was used.
fn explore_focus(
    project: &Path,
    options: &ExploreOptions,
    out_dir: &Path,
    kind: &str,
    out: &mut dyn Write,
) -> Result<
    (
        forge_uitest::focus::Focus,
        Option<(serde_json::Value, PathBuf)>,
    ),
    String,
> {
    use forge_uitest::focus::{Link, Strength};
    let aimed = options.since.is_some() || !options.changed.is_empty() || !options.focus.is_empty();
    // A plain walk learns from an earlier one only when asked to.
    if !aimed && options.previous.is_none() {
        return Ok((Default::default(), None));
    }
    let mut focus = forge_uitest::focus::Focus::default();
    let mut impact_json = serde_json::Value::Null;
    if options.since.is_some() || !options.changed.is_empty() {
        let (_, _, impact) =
            crate::focus::read(project, options.since.as_deref(), &options.changed)?;
        writeln!(
            out,
            "Aimed at {} changed file(s): {} identifier(s) declared, {} way(s) in, {} file(s) without a link",
            impact.changed.len(),
            impact.links.iter().filter(|l| l.strength == Strength::Direct).count(),
            impact.links.iter().filter(|l| l.strength == Strength::Entry).count(),
            impact.unmapped.len()
        )
        .map_err(err)?;
        focus = impact.focus();
        impact_json = impact.to_json();
    }
    for id in &options.focus {
        let prefix = id.ends_with(".*");
        focus.links.push(Link {
            id: id.trim_end_matches('*').to_string(),
            prefix,
            strength: Strength::Direct,
            file: String::new(),
            how: "given with --focus".into(),
        });
    }
    if aimed && focus.links.is_empty() {
        writeln!(
            out,
            "No identifier is tied to the change (see focus.json): the walk is the plain seeded one"
        )
        .map_err(err)?;
    }
    // Learning from the previous walk: its routes to the focus, and its
    // issues to check again.
    // Only a walk of the same kind: a model's issues say nothing about the
    // phone, and the phone's say nothing about a model.
    let previous = if options.no_previous {
        None
    } else if let Some(p) = &options.previous {
        let theirs = walk_kind(p);
        if theirs != kind {
            return Err(format!(
                "--previous {} is a {theirs} walk and this is a {kind} walk: their issues can't be compared (a fixture walk shows how Forge explores, not how the app behaves on the phone)",
                p.display()
            ));
        }
        Some(p.clone())
    } else {
        latest_walk(project, out_dir, kind)
    };
    let previous = match previous {
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("--previous {}: {e}", path.display()))?;
            let value: serde_json::Value = serde_json::from_str(&text)
                .map_err(|e| format!("--previous {}: {e}", path.display()))?;
            focus.routes = forge_uitest::focus::routes_from(&value, &focus, 6);
            writeln!(
                out,
                "Learning from {}: {} route(s) to follow first",
                path.display(),
                focus.routes.len()
            )
            .map_err(err)?;
            Some((value, path))
        }
        None => None,
    };
    std::fs::create_dir_all(out_dir).map_err(|e| format!("{}: {e}", out_dir.display()))?;
    let doc = serde_json::json!({
        "change": impact_json,
        "given": options.focus,
        "previous": previous.as_ref().map(|(_, p)| p.display().to_string()),
        "routes": focus.routes.iter().map(|r| serde_json::json!({
            "kind": r.kind, "target": r.target,
            "steps": r.actions.iter().map(|a| a.describe()).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    });
    std::fs::write(
        out_dir.join("focus.json"),
        serde_json::to_vec_pretty(&doc).unwrap_or_default(),
    )
    .map_err(|e| format!("{}: {e}", out_dir.display()))?;
    Ok((focus, previous))
}

/// The app's latest walk with an `explore.json`, other than `out_dir`.
/// `device` or `fixture`, from the `evidence.json` next to a walk's
/// `explore.json`; `unknown` without one.
fn walk_kind(explore_json: &Path) -> String {
    explore_json
        .parent()
        .map(|d| d.join("evidence.json"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        .and_then(|v| v["kind"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".into())
}

/// The newest earlier walk of the same kind (`device` or `fixture`).
fn latest_walk(project: &Path, out_dir: &Path, kind: &str) -> Option<PathBuf> {
    let runs = project.join(".forge/runs");
    let mut walks: Vec<PathBuf> = std::fs::read_dir(&runs)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("-ui-explore"))
        })
        .filter(|p| p != out_dir && p.join("explore.json").is_file())
        .filter(|p| walk_kind(&p.join("explore.json")) == kind)
        .collect();
    walks.sort();
    walks.pop().map(|p| p.join("explore.json"))
}

/// The newest earlier device walk launched with the same arguments and with
/// no error: what this walk's screenshots are compared with (M6.3).
fn last_clean_walk(
    project: &Path,
    out_dir: &Path,
    launch_args: &[String],
) -> Option<(PathBuf, serde_json::Value)> {
    let runs = project.join(".forge/runs");
    let mut walks: Vec<PathBuf> = std::fs::read_dir(&runs)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().ends_with("-ui-explore"))
        })
        .filter(|p| p != out_dir && walk_kind(&p.join("explore.json")) == "device")
        .collect();
    walks.sort();
    walks.into_iter().rev().find_map(|dir| {
        let text = std::fs::read_to_string(dir.join("explore.json")).ok()?;
        let value: serde_json::Value = serde_json::from_str(&text).ok()?;
        let same_launch = value["launch_args"].as_array().is_some_and(|a| {
            a.iter()
                .filter_map(|v| v.as_str())
                .eq(launch_args.iter().map(String::as_str))
        });
        let clean = !value["issues"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|i| i["severity"] == "error");
        (same_launch && clean).then_some((dir, value))
    })
}

/// `forge ui explore`: walk a Forge app's screens and report what is wrong.
/// Fails when the walk found an error (a crash, an unlabelled control).
pub fn explore(
    runner: &dyn CommandRunner,
    udid: Option<&str>,
    project: &Path,
    options: ExploreOptions,
    out_dir: Option<&Path>,
    out: &mut dyn Write,
) -> Result<(), String> {
    let mut config = explore_config(project, &options)?;
    let out_dir = out_dir.map(Path::to_path_buf).unwrap_or_else(|| {
        project
            .join(".forge/runs")
            .join(format!("{}-ui-explore", report::timestamp()))
    });
    let kind = if options.fixture.is_some() {
        "fixture"
    } else {
        "device"
    };
    let (focus, previous) = explore_focus(project, &options, &out_dir, kind, out)?;
    config.focus = focus;
    config.verify = options.verify;
    let started = std::time::Instant::now();
    // Read before the walk, so the bundle names the code that was explored.
    let mut build = crate::evidence::provenance(runner, project);
    let (report, watched, device) = match &options.fixture {
        // A model of the app: no phone, no watch, and the bundle says so.
        Some(model) => {
            let mut app = forge_uitest::model::Model::load(model)?;
            if app.app != config.script_app {
                return Err(format!(
                    "{} models {}, but the app's bundle id is {}",
                    model.display(),
                    app.app,
                    config.script_app
                ));
            }
            config.app = app.app.clone();
            writeln!(
                out,
                "Walking a model of the app ({}), not the phone{}",
                model.display(),
                app.note
                    .as_deref()
                    .map(|n| format!(": {n}"))
                    .unwrap_or_default()
            )
            .map_err(err)?;
            let report = explore::run(&config, &mut app, Some(&out_dir), out).map_err(err)?;
            let device = serde_json::json!({
                "fixture": model.display().to_string(),
                "synthesized_screens": app.synthesized,
            });
            let watched = crate::watch::Finished {
                value: crate::watch::not_watched("a model of the app: there is no phone to watch"),
                tail: Vec::new(),
                artifacts: Vec::new(),
            };
            (report, watched, device)
        }
        None => {
            // Reuses a running `forge ui start` session's runner when there
            // is one.
            let (phone, mut apps, held, mut driver) = connect(udid, out)?;
            config.app = resolve(&phone, &mut apps, &held, &config.script_app)?;
            let watcher = crate::watch::product(project)
                .map(|product| {
                    crate::watch::Watch::start(crate::watch::usb, &phone.udid, &product, project)
                })
                .map_err(|e| format!("the app's executable name isn't known ({e})"));
            let report = match explore::run(&config, &mut driver, Some(&out_dir), out) {
                Ok(report) => report,
                Err(e) => return Err(broke_off(err(e), watcher, &out_dir)),
            };
            let closed = report.issues.iter().any(|i| i.issue == "app_closed");
            let watched = match watcher {
                Ok(w) => w.finish(&out_dir, closed),
                Err(why) => crate::watch::Finished {
                    value: crate::watch::not_watched(&why),
                    tail: Vec::new(),
                    artifacts: Vec::new(),
                },
            };
            let device = serde_json::json!({
                "udid": phone.udid,
                "ios_major": phone.ios_major().ok(),
            });
            (report, watched, device)
        }
    };
    writeln!(out, "Report: {}", out_dir.join("explore.json").display()).map_err(err)?;
    // Screens against the last clean walk launched the same way (M6.3):
    // information for the reader, never an issue.
    let visual = match (kind, options.screenshots) {
        ("device", true) => match last_clean_walk(project, &out_dir, &report.launch_args) {
            Some((dir, earlier)) => {
                let screens: Vec<(String, Option<String>)> = report
                    .screens
                    .iter()
                    .map(|s| (s.id.clone(), s.screenshot.clone()))
                    .collect();
                let v = forge_uitest::baseline::compare_walks(&out_dir, &screens, &dir, &earlier);
                writeln!(
                    out,
                    "Against the last clean walk ({}): {} of {} screen(s) look different (not issues; see visual/)",
                    dir.display(),
                    v["changed"],
                    v["compared"]
                )
                .map_err(err)?;
                v
            }
            None => serde_json::json!({
                "compared": 0,
                "note": "No earlier clean walk launched the same way (same test data, appearance and text size) to compare screenshots with."
            }),
        },
        ("device", false) => serde_json::json!({"compared": 0, "note": "--no-screenshots"}),
        _ => serde_json::json!({"compared": 0, "note": "A model of the app has no screenshots."}),
    };
    // What changed since the previous walk: new, still there, gone (only
    // when this walk did what showed it), not rechecked.
    let compared = previous.as_ref().map(|(value, path)| {
        let mut c = forge_uitest::focus::compare(value, &path.display().to_string(), &report);
        // Both walks are of this kind (see explore_focus).
        c["kind"] = serde_json::json!(kind);
        c
    });
    if let Some(c) = &compared {
        let n = |k: &str| c[k].as_array().map_or(0, Vec::len);
        writeln!(
            out,
            "Against the previous walk: {} new, {} still there, {} gone, {} not rechecked",
            n("new"),
            n("still_present"),
            n("gone"),
            n("not_rechecked")
        )
        .map_err(err)?;
        let _ = std::fs::write(
            out_dir.join("compare.json"),
            serde_json::to_vec_pretty(c).unwrap_or_default(),
        );
    }
    if let Some(f) = &report.focus {
        let reached = f["targets"].as_array().map_or(0, Vec::len)
            - f["not_reached"].as_array().map_or(0, Vec::len);
        writeln!(
            out,
            "Focus: {reached} of {} declared identifier(s) reached, {} tried; first at step {}",
            f["targets"].as_array().map_or(0, Vec::len),
            f["tried"].as_object().map_or(0, |t| t.len()),
            f["first_direct_action"]
        )
        .map_err(err)?;
        writeln!(
            out,
            "  Ties to the change are read from the source as text: each issue's focus_basis says declared or inferred, never proven"
        )
        .map_err(err)?;
    }
    let summary = serde_json::json!({
        // `fixture`: a model of the app, no phone. It shows how Forge
        // explores and reports, never how the app behaves.
        "kind": kind,
        "app": report.app,
        "seed": report.seed,
        "stopped": report.stopped,
        "actions": report.actions,
        "screens": report.screens.len(),
        "issues": report.issues,
        "focus": report.focus,
        "compare": compared,
        "visual": visual,
    });
    build["installed_as"] = serde_json::json!(config.app);
    let tried = report
        .journal
        .iter()
        .filter(|s| s.action != "launch")
        .count();
    let evidence = crate::evidence::write_explore_bundle(
        &crate::evidence::ExploreRun {
            build,
            device,
            explore: summary.clone(),
            watch: Some(watched.value.clone()),
            artifacts: watched.artifacts,
            // A walk that tried nothing checked nothing: never a pass.
            ok: report.errors() == 0 && tried > 0,
            duration_ms: started.elapsed().as_millis() as u64,
            kind,
        },
        &out_dir,
        &report::timestamp(),
    )?;
    writeln!(out, "Evidence: {}", evidence.display()).map_err(err)?;
    if kind == "fixture" {
        writeln!(
            out,
            "Fixture walk: this shows how Forge explores and reports on a model, not how the app behaves on a phone. Its bundle is kind \"fixture\" and never counts as a device result."
        )
        .map_err(err)?;
    }
    let mut data = summary;
    data["report"] = serde_json::json!(out_dir.join("explore.json"));
    data["out"] = serde_json::json!(out_dir.display().to_string());
    data["evidence"] = serde_json::json!(evidence.display().to_string());
    report::set_data(data);
    if tried == 0 {
        return Err(format!(
            "exploring tried no control: {} screen(s) seen, every control skipped or unreachable (see skipped in {}). Nothing was checked",
            report.screens.len(),
            out_dir.join("explore.json").display()
        ));
    }
    match report.errors() {
        0 => Ok(()),
        n => Err(format!(
            "exploring found {n} error(s); each has a replay file under {}",
            out_dir.join("repro").display()
        )),
    }
}

/// The explorer's settings for an app Forge didn't build, at the level the
/// person's device-control policy gives it. Tapping needs `operate` or
/// `full`: at `operate` the walk only looks (`--real-data`), as on a
/// person's own data in a Forge app; at `full` the person has said the app
/// holds nothing they mind changing (a reference app subscribed to test
/// feeds), so it may save, edit and type as on test data.
fn policy_file() -> String {
    policy::policy_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|| "~/.config/forge/device-control.toml".into())
}

/// How a walk in another developer's app is set up from the person's policy
/// level. The levels mean what the any-app policy says they mean; nothing
/// here widens or redefines them.
pub fn foreign_explore_config(
    installed: &str,
    level: policy::Level,
    options: &ExploreOptions,
) -> Result<explore::Config, String> {
    let aimed = options.since.is_some()
        || !options.changed.is_empty()
        || !options.focus.is_empty()
        || options.previous.is_some()
        || options.fixture.is_some();
    if aimed {
        return Err(
            "--since, --changed, --focus, --previous and --fixture read a Forge app's source and earlier walks; they don't apply with --app"
                .into(),
        );
    }
    if options.appearance.is_some() {
        return Err(
            "--appearance works through -ForgeAppearance, which only Forge apps honour; set the phone's appearance instead"
                .into(),
        );
    }
    // The levels keep the meanings the any-app policy gives them
    // (forge_uitest::policy): the gate decides what each action may do, and
    // the walk's own data rules only narrow it further.
    let test_data = match level {
        policy::Level::Deny => {
            return Err(format!(
                "{installed} is at `deny` in your device-control policy ({}): Forge doesn't open it",
                policy_file()
            ))
        }
        // Launch, read, screenshot and scroll; the gate skips every tap and
        // all typing. Nothing in the app is touched, so --real-data is fine
        // either way.
        policy::Level::Observe => false,
        // Taps and typing on the person's own data, with each commit (send,
        // delete, a tap that says nothing about what it does) put to the
        // person. Ordinary taps can still change data, so the person says
        // they know: --real-data.
        policy::Level::Operate if !options.real_data => {
            return Err(format!(
                "{installed} is at `operate`: the walk taps and types in it on your own data, and ordinary taps can change that data (opening an article can mark it read). Pass --real-data to go ahead; commits still ask you each time"
            ))
        }
        policy::Level::Operate => false,
        // Everything but the hard refusals: only for an app whose data is all
        // throwaway test data. --real-data would not restrict the gate, so
        // it is refused rather than seeming to.
        policy::Level::Full if options.real_data => {
            return Err(format!(
                "{installed} is at `full`, which allows every action, so it is only for an app whose data is all test data; --real-data can't restrict it. Explore it at `operate` in {} to use your own data",
                policy_file()
            ))
        }
        policy::Level::Full => true,
    };
    if !test_data && options.allow_destructive {
        return Err(
            "--allow-destructive needs an app at `full`, whose data is all test data".into(),
        );
    }
    let mut fixtures = std::collections::HashMap::new();
    for pair in &options.texts {
        let (id, text) = pair
            .split_once('=')
            .ok_or_else(|| format!("--text {pair:?}: write IDENTIFIER=TEXT"))?;
        fixtures.insert(id.to_owned(), text.to_owned());
    }
    if options.actions < 2 {
        return Err("--actions must be at least 2".into());
    }
    if !(options.minutes > 0.0 && options.minutes.is_finite()) {
        return Err("--minutes must be more than 0".into());
    }
    let mut launch_args = Vec::new();
    if options.largest_text {
        launch_args.extend(explore::LARGEST_TEXT_ARGS.iter().map(|a| a.to_string()));
    }
    launch_args.extend(options.launch_args.iter().cloned());
    Ok(explore::Config {
        app: installed.to_owned(),
        script_app: installed.to_owned(),
        max_actions: options.actions,
        budget: Duration::from_secs_f64(options.minutes * 60.0),
        seed: options.seed.unwrap_or_else(|| {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(1)
        }),
        launch_args,
        fixtures,
        test_data,
        allow_destructive: options.allow_destructive,
        skip: options.skip.clone(),
        per_screen: options.per_screen.max(1),
        screenshots: options.screenshots,
        ..explore::Config::default()
    })
}

/// `forge ui explore --app <bundle id> --any-app`: walk an app Forge didn't
/// build (a reference app for a replication benchmark, docs/benchmarks/).
/// Every action goes through the any-app policy's gate and its journal;
/// screens go to a folder outside every project, because another app's
/// screens are the person's data. Findings in someone else's app are
/// information, so only a walk that tried nothing fails.
pub fn explore_app(
    udid: Option<&str>,
    app: &str,
    any_app: bool,
    options: ExploreOptions,
    out_dir: Option<&Path>,
    out: &mut dyn Write,
) -> Result<(), String> {
    if !any_app {
        return Err(format!(
            "{app} is not one of your Forge apps. Forge only explores apps it didn't build when the person passes --any-app, and then only as far as their device-control policy allows (docs/forge-explore.md#apps-forge-didnt-build)"
        ));
    }
    if let Some(why) = policy::never(app) {
        return Err(why);
    }
    let policy = Policy::load()?;
    let (phone, mut apps, held, mut driver) = connect(udid, out)?;
    let installed = resolve(&phone, &mut apps, &held, app)?;
    if let Some(why) = policy::never(&installed) {
        return Err(why);
    }
    let level = match policy.rule_for(&installed).level() {
        Some(level) => level,
        None => policy::Approver::choose(&mut TerminalApprover, &installed).ok_or_else(|| {
            format!(
                "{installed} is not in {} (default: ask), and there is nobody at a terminal to ask. Add a line for it, e.g. \"{installed}\" = \"operate\"",
                policy.where_()
            )
        })?,
    };
    let config = foreign_explore_config(&installed, level, &options)?;
    let dir = match out_dir {
        Some(d) => d.to_path_buf(),
        None => policy::session_root()
            .ok_or("no home folder for the any-app session; pass --out")?
            .join(format!("{}-ui-explore", report::timestamp())),
    };
    let journal = Journal::create(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    writeln!(
        out,
        "{installed} is not a Forge app: exploring at the `{}` level from {}, {}. Screens, the graph and the journal go to {}",
        level.name(),
        policy.where_(),
        match level {
            policy::Level::Observe => "looking only: launching, reading and scrolling; taps and typing are skipped",
            policy::Level::Operate => "on your own data: it taps rows, tabs, back and search, and those ordinary taps can change data (an article opened is marked read); every commit asks you first",
            _ => "and its data may change (the policy says it holds only test data)",
        },
        dir.display()
    )
    .map_err(err)?;
    let mut gate = Gate::foreign(&installed, level, Box::new(TerminalApprover), Some(journal));
    let report =
        explore::run_gated(&config, &mut driver, Some(&dir), out, &mut gate).map_err(err)?;
    writeln!(out, "Report: {}", dir.join("explore.json").display()).map_err(err)?;
    writeln!(out, "Graph: {}", dir.join("graph.json").display()).map_err(err)?;
    let tried = report
        .journal
        .iter()
        .filter(|s| s.action != "launch")
        .count();
    // Typed text the app echoed into an alert or label is taken out, as in
    // the files.
    let mut issues = serde_json::to_value(&report.issues).unwrap_or_default();
    report.scrub_json(&mut issues);
    report::set_data(serde_json::json!({
        "kind": "device",
        "any_app": { "app": installed, "level": level.name() },
        "app": report.app,
        "seed": report.seed,
        "stopped": report.stopped,
        "actions": report.actions,
        "screens": report.screens.len(),
        "issues": issues,
        "out": dir.display().to_string(),
        "report": dir.join("explore.json"),
        "graph": dir.join("graph.json"),
    }));
    // At `observe` taps and typing are refused by design: a walk that only
    // launched, read and scrolled did what was asked.
    if tried == 0 && level == policy::Level::Observe {
        writeln!(
            out,
            "Looked only (`observe`): taps and typing were skipped by the policy, as expected; each is listed under skipped in explore.json"
        )
        .map_err(err)?;
        return Ok(());
    }
    if tried == 0 {
        return Err(format!(
            "exploring tried no control: {} screen(s) seen, every control skipped or refused (see skipped in {})",
            report.screens.len(),
            dir.join("explore.json").display()
        ));
    }
    Ok(())
}

    fn options() -> ExploreOptions {
        ExploreOptions {
            since: None,
            changed: vec![],
            focus: vec![],
            previous: None,
            no_previous: false,
            verify: 3,
            fixture: None,
            actions: 60,
            minutes: 10.0,
            seed: Some(3),
            texts: vec!["bookForm.title=Dune".into()],
            skip: vec![],
            allow_destructive: false,
            real_data: false,
            largest_text: true,
            appearance: None,
            launch_args: vec![],
            per_screen: 15,
            screenshots: true,
        }
    }

    #[test]
    fn another_apps_walk_follows_the_persons_policy_level() {
        use forge_uitest::policy::Level;
        const NNW: &str = "com.ranchero.NetNewsWire-iOS";
        // The levels mean what the any-app policy says (forge_uitest::policy).
        // Deny: not opened.
        let e = foreign_explore_config(NNW, Level::Deny, &options()).unwrap_err();
        assert!(e.contains("`deny`"), "{e}");
        // Observe: a look-only walk; the gate skips taps and typing.
        let c = foreign_explore_config(NNW, Level::Observe, &options()).unwrap();
        assert!(!c.test_data);
        let real = ExploreOptions {
            real_data: true,
            ..options()
        };
        assert!(
            !foreign_explore_config(NNW, Level::Observe, &real)
                .unwrap()
                .test_data
        );
        // Operate: taps on the person's own data, so it needs --real-data,
        // and the refusal says ordinary taps can change data.
        let e = foreign_explore_config(NNW, Level::Operate, &options()).unwrap_err();
        assert!(e.contains("--real-data") && e.contains("can change"), "{e}");
        let c = foreign_explore_config(NNW, Level::Operate, &real).unwrap();
        assert!(!c.test_data);
        assert_eq!(c.app, NNW);
        // Full: the person said its data is all test data.
        let c = foreign_explore_config(NNW, Level::Full, &options()).unwrap();
        assert!(c.test_data);
        assert!(c
            .launch_args
            .iter()
            .any(|a| a.contains("ContentSizeCategory")));
        // Full with --real-data is refused: the flag can't restrict the gate.
        let e = foreign_explore_config(NNW, Level::Full, &real).unwrap_err();
        assert!(e.contains("operate"), "{e}");
        // Destructive taps only at full.
        let destructive = ExploreOptions {
            allow_destructive: true,
            ..options()
        };
        assert!(foreign_explore_config(NNW, Level::Full, &destructive).is_ok());
        let destructive_real = ExploreOptions {
            allow_destructive: true,
            ..real.clone()
        };
        assert!(foreign_explore_config(NNW, Level::Operate, &destructive_real).is_err());
        assert!(foreign_explore_config(NNW, Level::Observe, &destructive).is_err());
        // Options that read a Forge app's source don't apply.
        let aimed = ExploreOptions {
            since: Some("HEAD".into()),
            ..options()
        };
        assert!(foreign_explore_config(NNW, Level::Full, &aimed).is_err());
        let dark = ExploreOptions {
            appearance: Some("dark".into()),
            ..options()
        };
        assert!(foreign_explore_config(NNW, Level::Full, &dark).is_err());
    }

    fn pocket_library() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/pocket-library")
    }

    #[test]
    fn explore_reads_the_bundle_id_and_options() {
        let config = explore_config(&pocket_library(), &options()).unwrap();
        assert_eq!(config.script_app, "org.forgemobile.pocketlibrary");
        assert_eq!(config.seed, 3);
        assert_eq!(config.fixtures["bookForm.title"], "Dune");
        assert!(config.test_data);
        assert_eq!(
            config.launch_args,
            [
                "-ForgeUITestData",
                explore::LARGEST_TEXT_ARGS[0],
                explore::LARGEST_TEXT_ARGS[1]
            ]
        );
        assert_eq!(config.budget, Duration::from_secs(600));
        // --real-data: no test-data argument, and the walk only looks.
        let real = ExploreOptions {
            real_data: true,
            largest_text: false,
            ..options()
        };
        let config = explore_config(&pocket_library(), &real).unwrap();
        assert!(!config.test_data);
        assert!(config.launch_args.is_empty());
        // Dark mode is passed on every launch, like the text size.
        let dark = ExploreOptions {
            appearance: Some("dark".into()),
            ..options()
        };
        let config = explore_config(&pocket_library(), &dark).unwrap();
        assert_eq!(
            config.launch_args[..3],
            ["-ForgeUITestData", "-ForgeAppearance", "dark"]
        );
        // The app's own arguments come after Forge's.
        let own = ExploreOptions {
            launch_args: vec!["-GoodDogSampleDog".into()],
            largest_text: false,
            ..options()
        };
        let config = explore_config(&pocket_library(), &own).unwrap();
        assert_eq!(config.launch_args.last().unwrap(), "-GoodDogSampleDog");
        let reckless = ExploreOptions {
            allow_destructive: true,
            ..real
        };
        assert!(explore_config(&pocket_library(), &reckless).is_err());
    }

    #[test]
    fn explore_refuses_options_that_would_check_nothing() {
        for bad in [
            ExploreOptions {
                actions: 1,
                ..options()
            },
            ExploreOptions {
                minutes: 0.0,
                ..options()
            },
            ExploreOptions {
                texts: vec!["no-equals".into()],
                ..options()
            },
        ] {
            assert!(explore_config(&pocket_library(), &bad).is_err());
        }
    }

    #[test]
    fn explore_refuses_an_app_without_test_data() {
        let dir = tempfile::tempdir().unwrap();
        let forge = std::fs::read_to_string(pocket_library().join("forge.toml")).unwrap();
        let without = forge.split("\n# UI automation.").next().unwrap();
        assert!(!without.contains("[ui_test]"));
        std::fs::write(dir.path().join("forge.toml"), without).unwrap();
        let err = explore_config(dir.path(), &options()).unwrap_err();
        assert!(err.contains("test-data mode"), "{err}");
        assert!(err.contains("--real-data"), "{err}");
        let real = ExploreOptions {
            real_data: true,
            ..options()
        };
        assert!(!explore_config(dir.path(), &real).unwrap().test_data);
        // Controls the developer declared safe on real data reach the walk.
        std::fs::write(
            dir.path().join("forge.toml"),
            format!("{without}\n[ui_test]\nreal_data_safe = [\"library.settings\"]\n"),
        )
        .unwrap();
        let config = explore_config(dir.path(), &real).unwrap();
        assert!(!config.test_data);
        assert_eq!(config.real_data_safe, vec!["library.settings".to_owned()]);
    }

    #[test]
    fn explore_needs_a_forge_app_and_well_formed_options() {
        let dir = tempfile::tempdir().unwrap();
        let err = explore_config(dir.path(), &options()).unwrap_err();
        assert!(err.contains("--project"), "{err}");
        let bad = ExploreOptions {
            texts: vec!["no-equals".into()],
            ..options()
        };
        let err = explore_config(&pocket_library(), &bad).unwrap_err();
        assert!(err.contains("IDENTIFIER=TEXT"), "{err}");
        let bad = ExploreOptions {
            minutes: 0.0,
            ..options()
        };
        assert!(explore_config(&pocket_library(), &bad).is_err());
    }

    #[test]
    fn a_walk_learns_only_from_walks_of_its_own_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path().join(".forge/runs");
        let walk = |name: &str, kind: Option<&str>| {
            let dir = runs.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("explore.json"), "{}").unwrap();
            if let Some(kind) = kind {
                std::fs::write(
                    dir.join("evidence.json"),
                    serde_json::json!({"kind": kind}).to_string(),
                )
                .unwrap();
            }
            dir
        };
        let device = walk("2026-11-08T10-00-00-ui-explore", Some("device"));
        // Newer, but a model of the app: never the previous of a phone walk.
        let fixture = walk("2026-11-08T11-00-00-ui-explore", Some("fixture"));
        walk("2026-11-08T12-00-00-ui-explore", None);
        let now = runs.join("2026-11-08T13-00-00-ui-explore");
        assert_eq!(
            latest_walk(tmp.path(), &now, "device"),
            Some(device.join("explore.json"))
        );
        assert_eq!(
            latest_walk(tmp.path(), &now, "fixture"),
            Some(fixture.join("explore.json"))
        );
        assert_eq!(walk_kind(&fixture.join("explore.json")), "fixture");
        // Named by hand, a walk of the other kind is refused, not compared.
        let options = ExploreOptions {
            previous: Some(fixture.join("explore.json")),
            ..options()
        };
        let mut out = Vec::new();
        let refused = explore_focus(tmp.path(), &options, &now, "device", &mut out).unwrap_err();
        assert!(
            refused.contains("is a fixture walk and this is a device walk"),
            "{refused}"
        );
    }

    #[test]
    fn screenshots_are_compared_with_the_last_clean_walk_launched_the_same_way() {
        let tmp = tempfile::tempdir().unwrap();
        let runs = tmp.path().join(".forge/runs");
        let walk = |name: &str, kind: &str, args: &[&str], error: bool| {
            let dir = runs.join(name);
            std::fs::create_dir_all(&dir).unwrap();
            let issues = if error {
                serde_json::json!([{"severity": "error"}])
            } else {
                serde_json::json!([{"severity": "warning"}])
            };
            std::fs::write(
                dir.join("explore.json"),
                serde_json::json!({"launch_args": args, "issues": issues, "screens": []})
                    .to_string(),
            )
            .unwrap();
            std::fs::write(
                dir.join("evidence.json"),
                serde_json::json!({"kind": kind}).to_string(),
            )
            .unwrap();
            dir
        };
        let light = walk(
            "2026-11-08T09-00-00-ui-explore",
            "device",
            &["-ForgeUITestData"],
            false,
        );
        let dark = walk(
            "2026-11-08T10-00-00-ui-explore",
            "device",
            &["-ForgeUITestData", "-ForgeAppearance", "dark"],
            false,
        );
        // Newer, but with a crash: not clean.
        walk(
            "2026-11-08T11-00-00-ui-explore",
            "device",
            &["-ForgeUITestData"],
            true,
        );
        // Newer, but a model.
        walk(
            "2026-11-08T12-00-00-ui-explore",
            "fixture",
            &["-ForgeUITestData"],
            false,
        );
        let now = runs.join("2026-11-08T13-00-00-ui-explore");
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            last_clean_walk(tmp.path(), &now, &args(&["-ForgeUITestData"])).map(|(d, _)| d),
            Some(light)
        );
        assert_eq!(
            last_clean_walk(
                tmp.path(),
                &now,
                &args(&["-ForgeUITestData", "-ForgeAppearance", "dark"])
            )
            .map(|(d, _)| d),
            Some(dark)
        );
        assert!(last_clean_walk(tmp.path(), &now, &args(&["-ForgeAppearance", "light"])).is_none());
    }
