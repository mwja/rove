use std::{
    fmt::Write as _,
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
    process::Command,
    sync::Once,
};

fn build_runtime() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let status = Command::new(env!("CARGO"))
            .args([
                "build",
                "--release",
                "--manifest-path",
                "runtime/Cargo.toml",
            ])
            .status()
            .unwrap();
        assert!(status.success(), "failed to build runtime");
    });
}

fn render(path: &Path) -> String {
    let rel = path
        .strip_prefix(env!("CARGO_MANIFEST_DIR"))
        .unwrap()
        .to_path_buf();
    let out_dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(rel.parent().unwrap());
    std::fs::create_dir_all(&out_dir).unwrap();
    let out: PathBuf = out_dir.join(rel.file_stem().unwrap());

    let mut source_map = rove::SourceMap::new();
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        rove::compile(rel.clone(), out.clone(), &[], &mut source_map)
    }));

    match result {
        Err(payload) => {
            let msg = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_default();
            format!("compiler panicked: {msg}\n")
        }
        Ok(Err(diagnostics)) => {
            let builder = source_map.build_report().with_diagnostics(diagnostics);
            let mut buf = Vec::new();
            for report in builder.build() {
                report.write(builder.as_cache(), &mut buf).unwrap();
            }
            format!("compile error:\n{}", String::from_utf8(buf).unwrap())
        }
        Ok(Ok(())) => {
            let output = Command::new(&out).output().unwrap();
            let mut s = format!("exit: {:?}\n", output.status.code());
            write!(s, "--- stdout\n{}", String::from_utf8_lossy(&output.stdout)).unwrap();
            if !output.stderr.is_empty() {
                write!(s, "--- stderr\n{}", String::from_utf8_lossy(&output.stderr)).unwrap();
            }
            s
        }
    }
}

fn snapshot_dir(base: &str, snapshots: &str) {
    build_runtime();
    let mut settings = insta::Settings::clone_current();
    settings.add_filter(r"\x1b\[[0-9;]*m", "");
    settings.add_filter(r"\\", "/");
    settings.set_prepend_module_to_snapshot(false);
    settings.set_omit_expression(true);
    settings.set_snapshot_path(format!("snapshots/{snapshots}"));
    settings.bind(|| {
        insta::glob!(base, "*.rv", |path| {
            insta::assert_snapshot!("run", render(path));
        });
    });
}

#[test]
fn cases() {
    snapshot_dir("cases", "cases");
}
