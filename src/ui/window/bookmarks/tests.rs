// SPDX-License-Identifier: MIT

use super::*;

#[test]
fn deleted_pins_and_descendants_are_removed_without_rewriting_other_entries() {
    let contents = b"file:///fixture/gone Gone\r\n\
        file:///fixture/gone/child Child\n\
        file:///fixture/gone%20too Keep\n\
        file:///fixture/gone-sibling Sibling\n\
        file:///disconnected/drive Offline\n\
        smb://unavailable/share Network\n\
        file:///fixture/keep Raw\xff\r\n\
        \xffinvalid\n\n\
        file:///fixture/final No newline";
    let expected = b"file:///fixture/gone%20too Keep\n\
        file:///fixture/gone-sibling Sibling\n\
        file:///disconnected/drive Offline\n\
        smb://unavailable/share Network\n\
        file:///fixture/keep Raw\xff\r\n\
        \xffinvalid\n\n\
        file:///fixture/final No newline";
    let deleted = [gio::File::for_path("/fixture/gone")];
    assert_eq!(retain_unrelated_bookmarks(contents, &deleted), expected);
    assert_eq!(retain_unrelated_bookmarks(contents, &[]), contents);
}

#[test]
fn cleanup_reads_current_bookmarks_and_leaves_missing_files_absent() {
    let directory = tempfile::tempdir().expect("fixture");
    let path = directory.path().join("bookmarks");
    let file = gio::File::for_path(&path);
    let deleted = [Location::local("/fixture/gone")];
    remove_deleted_pins(&file, &deleted).expect("missing bookmarks");
    assert!(!path.exists());
    std::fs::write(&path, b"file:///fixture/gone Gone\n").expect("initial pins");
    std::fs::write(
        &path,
        b"file:///fixture/gone Gone\nfile:///fixture/added External\xff\n",
    )
    .expect("external edit");
    remove_deleted_pins(&file, &deleted).expect("cleanup");
    assert_eq!(
        std::fs::read(&path).expect("saved bookmarks"),
        b"file:///fixture/added External\xff\n"
    );
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !condition() {
        assert!(std::time::Instant::now() < deadline, "operation timed out");
        glib::MainContext::default().iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn deletion_updates_both_sidebars(permanent: bool) {
    let home = std::path::PathBuf::from(std::env::var_os("HOME").expect("isolated home"));
    let parent = home.join("fixture/gone");
    let child = parent.join("child");
    let keep = home.join("fixture/keep");
    std::fs::create_dir_all(&child).expect("deleted folder");
    std::fs::create_dir_all(&keep).expect("retained folder");
    let initial = vec![
        (Location::local(&parent), "Gone".into()),
        (Location::local(&child), "Child".into()),
        (Location::local(&keep), "Keep".into()),
    ];
    super::super::save_pinned_places(&initial).expect("pins");
    let preferences = crate::ui::preferences::PreferenceManager::shared();
    let first = super::super::browser_for_window();
    let second = super::super::browser_for_window();
    let first_sidebar = super::super::build_sidebar(first.clone(), preferences.clone(), true);
    let second_sidebar = super::super::build_sidebar(second, preferences, true);
    let browser = first.browser();
    browser.navigate(Location::local(parent.parent().expect("parent")));
    wait_until(|| {
        browser
            .column_snapshot(0)
            .is_some_and(|column| !column.loading)
    });
    browser.select_entries_by_name(&["gone".into()]);
    let entries = browser.selected_entries();
    assert_eq!(entries.len(), 1);
    let finished = Rc::new(RefCell::new(None));
    let observed = finished.clone();
    browser.observe(move |event| {
        if let crate::app::BrowserEvent::DeletionFinished { succeeded } = event {
            observed.replace(Some(*succeeded));
        }
    });
    browser.delete(entries, permanent);
    wait_until(|| finished.borrow().is_some());
    assert_eq!(*finished.borrow(), Some(true));
    assert!(!parent.exists());
    let expected = vec![(Location::local(&keep), "Keep".into())];
    assert_eq!(load_pinned_places().expect("saved pins"), expected);
    assert_eq!(*first_sidebar.state.pinned_places.borrow(), expected);
    assert_eq!(*second_sidebar.state.pinned_places.borrow(), expected);
    assert_eq!(
        first_sidebar.state.visible_pins.borrow().as_slice(),
        &[Location::local(&keep)]
    );
    assert_eq!(
        second_sidebar.state.visible_pins.borrow().as_slice(),
        &[Location::local(&keep)]
    );
    if !permanent {
        let restored = Rc::new(std::cell::Cell::new(false));
        let observed = restored.clone();
        browser.observe(move |event| {
            if matches!(event, crate::app::BrowserEvent::RestorationFinished) {
                observed.set(true);
            }
        });
        assert!(first.undo_last_operation());
        wait_until(|| restored.get());
        assert!(parent.exists());
        assert_eq!(load_pinned_places().expect("pins after restore"), expected);
    }
}

#[test]
fn permanent_deletion_updates_pins_in_all_sidebars() {
    crate::test_support::gtk_test(
        "ui::window::bookmarks::tests::permanent_deletion_updates_pins_in_all_sidebars",
        || deletion_updates_both_sidebars(true),
    );
}

#[test]
fn trash_and_restore_do_not_resurrect_removed_pins() {
    crate::test_support::gtk_test(
        "ui::window::bookmarks::tests::trash_and_restore_do_not_resurrect_removed_pins",
        || deletion_updates_both_sidebars(false),
    );
}
