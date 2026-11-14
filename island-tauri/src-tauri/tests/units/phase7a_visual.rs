#![allow(clippy::unwrap_used)]
use hermes_island_lib::{
    contract::{frontend::Appearance, island::UiKind},
    services::{frontend::VisualService, ServiceConfig, Services},
};
use std::{fs, path::PathBuf};
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn appearance_is_atomic_private_persistent_and_visual_revisions_never_regress() {
    let dir = Scratch(std::env::temp_dir().join(format!("hi-7a-visual-{}", std::process::id())));
    let root = dir.0.join("repo");
    let home = dir.0.join("home");
    let data = dir.0.join("data");
    fs::create_dir_all(root.join("island")).unwrap();
    let launcher = home.join(".hermes/hermes-agent/.hermes/bin/hermes");
    fs::create_dir_all(launcher.parent().unwrap()).unwrap();
    fs::write(launcher, "exec /usr/bin/python3\n").unwrap();
    fs::write(root.join("island/palette.py"), "print('{\"name\":\"fixture\",\"background\":\"#111111\",\"accent\":\"#91c99b\",\"text\":\"#dddddd\",\"primary\":\"#eeeeee\",\"dim\":\"#999999\",\"border\":\"#444444\",\"surface\":\"#222222\",\"error\":\"#ff8888\"}')").unwrap();
    let services = Services::new(&ServiceConfig {
        repo_root: root,
        data_dir: data.clone(),
        config_dir: data.clone(),
        home,
        overlay: true,
        test_instance: true,
        fixture_pty: true,
        ui: UiKind::Island,
        test_hooks: false,
    });
    let visual = VisualService::new(data.clone()).unwrap();
    let first = visual.snapshot(&services, None).unwrap();
    assert_eq!(first.appearance, Appearance::Skin);
    let neutral = visual
        .snapshot(&services, Some(Appearance::Neutral))
        .unwrap();
    assert!(neutral.rev > first.rev);
    let restarted = VisualService::new(data.clone())
        .unwrap()
        .snapshot(&services, None)
        .unwrap();
    assert_eq!(
        restarted.appearance,
        Appearance::Neutral,
        "appearance choice did not survive restart"
    );
    assert_eq!(
        fs::read(data.join("appearance.json")).unwrap(),
        b"\"neutral\""
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        fs::metadata(data.join("appearance.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    services
        .settings
        .patch(&serde_json::from_str("{\"uiFont\":\"openrunde\"}").unwrap())
        .unwrap();
    let changed = visual.snapshot(&services, None).unwrap();
    assert!(changed.rev > neutral.rev);
    assert_eq!(changed.settings.ui_font.as_str(), "openrunde");
    assert_eq!(visual.snapshot(&services, None).unwrap().rev, changed.rev);
    // Failed atomic replacement must leave the in-memory choice unchanged.
    fs::remove_file(data.join("appearance.json")).unwrap();
    fs::create_dir(data.join("appearance.json")).unwrap();
    assert!(visual.snapshot(&services, Some(Appearance::Skin)).is_err());
    assert_eq!(
        visual.snapshot(&services, None).unwrap().appearance,
        Appearance::Neutral
    );
    services.shutdown();
}
