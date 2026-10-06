// SPDX-License-Identifier: MIT

use super::*;
use crate::test_support::gtk_test;

fn open() -> (gtk::ApplicationWindow, Rc<TabWindow>) {
    let application = gtk::Application::new(None::<&str>, gio::ApplicationFlags::NON_UNIQUE);
    application
        .register(None::<&gio::Cancellable>)
        .expect("test application");
    let preferences = PreferenceManager::shared();
    preferences.set_tenxer_mode(false);
    let window = gtk::ApplicationWindow::builder()
        .application(&application)
        .default_width(1000)
        .default_height(700)
        .build();
    let tabs = TabWindow::new(&window, &preferences);
    window.present();
    (window, tabs)
}

fn wait_until(condition: impl Fn() -> bool) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !condition() {
        assert!(
            std::time::Instant::now() < deadline,
            "tab state did not settle"
        );
        while glib::MainContext::default().pending() {
            glib::MainContext::default().iteration(false);
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

fn load(browser: &BrowserView, location: Location) {
    browser.navigate_location(location);
    wait_until(|| {
        browser
            .browser()
            .column_snapshot(0)
            .is_some_and(|column| !column.loading)
    });
}

#[test]
fn tabs_preserve_locations_and_route_actions_to_the_active_browser() {
    gtk_test(
        "ui::window::composition::tabs::tests::tabs_preserve_locations_and_route_actions_to_the_active_browser",
        || {
            let root = tempfile::tempdir().expect("tab fixture");
            std::fs::create_dir(root.path().join("other")).expect("second directory");
            std::fs::write(root.path().join("one.txt"), "one").expect("first file");
            let (window, tabs) = open();
            let first = tabs.active_browser();
            load(&first, Location::local(root.path()));
            first.browser().select(0, 1);
            let selected: Vec<_> = first
                .browser()
                .selected_entries()
                .into_iter()
                .map(|entry| entry.location)
                .collect();
            tabs.new_tab();
            let second_id = tabs.active.get();
            let second = tabs.active_browser();
            load(&second, Location::local(root.path().join("other")));
            tabs.select(1);
            assert_eq!(
                first.browser().active_location(),
                Some(Location::local(root.path()))
            );
            assert_eq!(
                first
                    .browser()
                    .selected_entries()
                    .into_iter()
                    .map(|entry| entry.location)
                    .collect::<Vec<_>>(),
                selected
            );
            std::fs::write(root.path().join("other/two.txt"), "two").expect("refresh fixture");
            tabs.select(second_id);
            gio::prelude::ActionGroupExt::activate_action(&window, "refresh", None);
            wait_until(|| {
                second
                    .browser()
                    .column_snapshot(0)
                    .is_some_and(|column| !column.loading && column.count == 1)
            });
            assert_eq!(
                first.browser().active_location(),
                Some(Location::local(root.path()))
            );
            window.destroy();
        },
    );
}

#[test]
fn tab_shortcuts_work_in_both_modes_and_follow_reordering() {
    gtk_test(
        "ui::window::composition::tabs::tests::tab_shortcuts_work_in_both_modes_and_follow_reordering",
        || {
            use gdk::{Key, ModifierType as M};
            let (window, tabs) = open();
            let root = tempfile::tempdir().expect("tab fixture");
            load(&tabs.active_browser(), Location::local(root.path()));
            for tenxer in [false, true] {
                tabs.preferences.set_tenxer_mode(tenxer);
                for key in [Key::Page_Up, Key::Page_Down] {
                    assert!(is_tab_shortcut(key, M::CONTROL_MASK));
                    assert_eq!(
                        tabs.handle_key(key, M::CONTROL_MASK),
                        glib::Propagation::Stop
                    );
                    assert_eq!(tabs.active.get(), 1);
                }
                assert_eq!(
                    tabs.handle_key(Key::t, M::CONTROL_MASK),
                    glib::Propagation::Stop
                );
                let new_id = tabs.active.get();
                tabs.handle_key(Key::exclam, M::CONTROL_MASK | M::SHIFT_MASK);
                assert_eq!(tabs.active.get(), 1);
                tabs.reorder(new_id, 1);
                tabs.handle_key(Key::exclam, M::CONTROL_MASK | M::SHIFT_MASK);
                assert_eq!(tabs.active.get(), new_id);
                tabs.handle_key(Key::Tab, M::CONTROL_MASK);
                assert_eq!(tabs.active.get(), 1);
                tabs.handle_key(Key::ISO_Left_Tab, M::CONTROL_MASK | M::SHIFT_MASK);
                assert_eq!(tabs.active.get(), new_id);
                tabs.new_tab();
                let last_id = tabs.active.get();
                tabs.select(new_id);
                for (key, expected) in [
                    (Key::Page_Up, last_id),
                    (Key::Page_Up, 1),
                    (Key::Page_Down, last_id),
                    (Key::Page_Down, new_id),
                    (Key::KP_Page_Up, last_id),
                    (Key::KP_Page_Up, 1),
                    (Key::KP_Page_Down, last_id),
                    (Key::KP_Page_Down, new_id),
                ] {
                    assert!(is_tab_shortcut(key, M::CONTROL_MASK | M::LOCK_MASK));
                    assert_eq!(
                        tabs.handle_key(key, M::CONTROL_MASK | M::LOCK_MASK),
                        glib::Propagation::Stop
                    );
                    assert_eq!(tabs.active.get(), expected, "{key:?}, 10xer={tenxer}");
                }
                for modifiers in [
                    M::empty(),
                    M::SUPER_MASK,
                    M::CONTROL_MASK | M::SHIFT_MASK,
                    M::CONTROL_MASK | M::ALT_MASK,
                    M::CONTROL_MASK | M::SUPER_MASK,
                    M::CONTROL_MASK | M::META_MASK,
                    M::CONTROL_MASK | M::HYPER_MASK,
                ] {
                    for key in [
                        Key::Page_Up,
                        Key::Page_Down,
                        Key::KP_Page_Up,
                        Key::KP_Page_Down,
                    ] {
                        assert!(!is_tab_shortcut(key, modifiers));
                        assert_eq!(tabs.handle_key(key, modifiers), glib::Propagation::Proceed);
                        assert_eq!(tabs.active.get(), new_id);
                    }
                }
                tabs.close(last_id);
                tabs.handle_key(Key::w, M::CONTROL_MASK);
                assert_eq!(tabs.active.get(), 1);
            }
            window.destroy();
        },
    );
}

#[test]
fn closing_tabs_releases_observers_and_keeps_other_contexts_alive() {
    gtk_test(
        "ui::window::composition::tabs::tests::closing_tabs_releases_observers_and_keeps_other_contexts_alive",
        || {
            let (window, tabs) = open();
            let root = tempfile::tempdir().expect("tab fixture");
            load(&tabs.active_browser(), Location::local(root.path()));
            tabs.new_tab();
            let closed = tabs.active_browser().downgrade();
            let id = tabs.active.get();
            tabs.close(id);
            wait_until(|| closed.upgrade().is_none());
            assert_eq!(tabs.active.get(), 1);
            assert_eq!(
                tabs.active_browser().browser().active_location(),
                Some(Location::local(root.path()))
            );
            tabs.new_tab();
            let active = tabs.active.get();
            tabs.close(1);
            assert_eq!(tabs.active.get(), active);
            assert_eq!(tabs.tabs.borrow().len(), 1);
            window.destroy();
        },
    );
}

#[test]
fn hidden_tabs_and_new_tabs_apply_live_browsing_preferences() {
    gtk_test(
        "ui::window::composition::tabs::tests::hidden_tabs_and_new_tabs_apply_live_browsing_preferences",
        || {
            let root = tempfile::tempdir().expect("tab fixture");
            std::fs::write(root.path().join("visible.txt"), "visible").expect("visible file");
            std::fs::write(root.path().join(".hidden.txt"), "hidden").expect("hidden file");
            let (window, tabs) = open();
            let mut initial = tabs.preferences.sort_preferences();
            initial.show_hidden = false;
            tabs.preferences.set_sort_preferences(initial);
            let first = tabs.active_browser();
            load(&first, Location::local(root.path()));
            tabs.new_tab();
            let second = tabs.active_browser();
            wait_until(|| {
                second
                    .browser()
                    .column_snapshot(0)
                    .is_some_and(|column| !column.loading)
            });
            assert_eq!(
                first
                    .browser()
                    .column_entry_counts(0)
                    .expect("first tab loaded")
                    .total,
                1
            );
            assert_eq!(
                second
                    .browser()
                    .column_entry_counts(0)
                    .expect("second tab loaded")
                    .total,
                1
            );
            let mut preferences = tabs.preferences.sort_preferences();
            preferences.show_hidden = true;
            tabs.preferences.set_sort_preferences(preferences);
            wait_until(|| {
                [first.clone(), second.clone()].iter().all(|view| {
                    view.browser()
                        .column_entry_counts(0)
                        .is_some_and(|counts| counts.total == 2)
                })
            });
            tabs.new_tab();
            wait_until(|| {
                tabs.active_browser()
                    .browser()
                    .column_entry_counts(0)
                    .is_some_and(|counts| counts.total == 2)
            });
            tabs.select(1);
            assert_eq!(
                tabs.active_browser()
                    .browser()
                    .column_entry_counts(0)
                    .expect("first tab retained")
                    .total,
                2
            );
            window.destroy();
        },
    );
}

#[test]
fn operations_in_inactive_tabs_prevent_tab_and_window_closure() {
    gtk_test(
        "ui::window::composition::tabs::tests::operations_in_inactive_tabs_prevent_tab_and_window_closure",
        || {
            use crate::{
                services::{PasteItem, TransferConflict},
                test_support::operations::HeldOperations,
            };
            let (window, tabs) = open();
            let root = tempfile::tempdir().expect("tab fixture");
            let browser = tabs.active_browser().browser();
            load(&tabs.active_browser(), Location::local(root.path()));
            let operations = Rc::new(HeldOperations::default());
            browser.set_operation_provider(operations.clone());
            browser.transfer(
                Location::local(root.path().join("destination")),
                vec![PasteItem {
                    source: Location::local(root.path().join("source")),
                    conflict: TransferConflict::FailIfExists,
                }],
                false,
                false,
            );
            let operation = browser.last_started_operation().expect("transfer started");
            assert!(browser.has_background_operations());
            tabs.new_tab();
            assert_eq!(tabs.tabs.borrow().len(), 2);
            tabs.close(1);
            assert_eq!(tabs.tabs.borrow().len(), 2);
            assert!(!operations.cancelled(operation));
            window.close();
            assert!(window.is_visible());
            assert!(!operations.cancelled(operation));
            let count = tabs.tabs.borrow().len();
            tabs.new_tab();
            assert_eq!(
                tabs.tabs.borrow().len(),
                count,
                "modal input cannot create a hidden tab"
            );
            let active = tabs.active.get();
            for key in [
                gdk::Key::Page_Up,
                gdk::Key::Page_Down,
                gdk::Key::KP_Page_Up,
                gdk::Key::KP_Page_Down,
            ] {
                assert_eq!(
                    tabs.handle_key(key, gdk::ModifierType::CONTROL_MASK),
                    glib::Propagation::Proceed
                );
                assert_eq!(tabs.active.get(), active, "modal input cannot switch tabs");
            }
            window.destroy();
            assert!(operations.cancelled(operation));
        },
    );
}
