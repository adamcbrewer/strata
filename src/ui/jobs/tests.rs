// SPDX-License-Identifier: MIT

use super::*;
use crate::model::ExecutionMode;
use crate::services::jobs::JobProgress;

#[test]
fn job_output_hides_terminal_formatting_without_losing_unicode() {
    assert_eq!(
        strip_terminal_sequences("\x1b[0mRésumé \x1b[38;5;214mready\x1b[0m\n"),
        "Résumé ready\n"
    );
    assert_eq!(
        strip_terminal_sequences(
            "\x1b]0;private title\x07next\x1b]8;;https://example.org\x1b\\link\x1b]8;;\x1b\\"
        ),
        "nextlink"
    );
    assert_eq!(strip_terminal_sequences("partial \x1b[38;5"), "partial ");
}

#[test]
fn job_output_follows_new_lines_until_scrolled_up() {
    crate::test_support::gtk_test(
        "ui::jobs::tests::job_output_follows_new_lines_until_scrolled_up",
        || {
            let label = gtk::Label::new(Some(&"line\n".repeat(80)));
            let scroll = gtk::ScrolledWindow::builder()
                .child(&label)
                .max_content_height(160)
                .propagate_natural_height(true)
                .build();
            follow_log_output(&scroll);
            let window = gtk::Window::builder().child(&scroll).build();
            window.present();
            let adjustment = scroll.vadjustment();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while adjustment.upper() <= adjustment.page_size() {
                glib::MainContext::default().iteration(false);
                assert!(
                    std::time::Instant::now() < deadline,
                    "output did not become scrollable"
                );
            }
            assert!(at_log_bottom(&adjustment));

            let prior_upper = adjustment.upper();
            label.set_text(&"line\n".repeat(100));
            while adjustment.upper() <= prior_upper && std::time::Instant::now() < deadline {
                glib::MainContext::default().iteration(false);
            }
            assert!(adjustment.upper() > prior_upper, "new output was laid out");
            assert!(at_log_bottom(&adjustment));

            adjustment.set_value(adjustment.lower());
            let prior_upper = adjustment.upper();
            label.set_text(&"line\n".repeat(120));
            while adjustment.upper() <= prior_upper && std::time::Instant::now() < deadline {
                glib::MainContext::default().iteration(false);
            }
            assert!(
                adjustment.upper() > prior_upper,
                "later output was laid out"
            );
            assert!(!at_log_bottom(&adjustment), "scrolling up pauses following");

            adjustment.set_value(adjustment.upper() - adjustment.page_size());
            let prior_upper = adjustment.upper();
            label.set_text(&"line\n".repeat(140));
            while adjustment.upper() <= prior_upper && std::time::Instant::now() < deadline {
                glib::MainContext::default().iteration(false);
            }
            assert!(
                at_log_bottom(&adjustment),
                "returning to the bottom resumes following"
            );
            window.close();
        },
    );
}

#[test]
fn running_job_details_can_be_opened_and_stay_open_after_completion() {
    crate::test_support::gtk_test(
        "ui::jobs::tests::running_job_details_can_be_opened_and_stay_open_after_completion",
        || {
            let state = DashboardState {
                service: JobService::new(crate::adapters::LocalActionRunner::new()),
                expanded: Rc::new(RefCell::new(HashSet::new())),
                dirty: Rc::new(RefCell::new(false)),
                featured: Rc::new(Cell::new(None)),
                status_labels: Rc::new(RefCell::new(Vec::new())),
                rows: Rc::new(RefCell::new(HashMap::new())),
            };
            let snapshot = JobSnapshot {
                id: JobId(1),
                action_name: "Action".to_owned(),
                icon: None,
                mode: ExecutionMode::WholeSelection,
                parent: std::path::PathBuf::from("/tmp"),
                status: JobStatus::Running,
                progress: JobProgress::default(),
                log: "first line\n".to_owned(),
                log_truncated: false,
                created: Vec::new(),
                message: None,
                elapsed: Duration::ZERO,
            };
            let refresh: RefreshCallback = Rc::new(|| {});
            let controls = job_controls(&snapshot, &state, &refresh);
            let details = controls
                .iter()
                .find(|button| button.tooltip_text().as_deref() == Some("Details"))
                .expect("running job offers Details");
            details.emit_clicked();
            assert!(state.expanded.borrow().contains(&snapshot.id));

            let finished = JobSnapshot {
                status: JobStatus::Succeeded,
                log: "first line\nlast line\n".to_owned(),
                ..snapshot
            };
            let controls = job_controls(&finished, &state, &refresh);
            assert!(
                controls
                    .iter()
                    .any(|button| button.tooltip_text().as_deref() == Some("Hide"))
            );
        },
    );
}
