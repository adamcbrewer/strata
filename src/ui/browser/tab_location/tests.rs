// SPDX-License-Identifier: MIT

use std::cell::RefCell;

use gtk::prelude::*;

use super::*;
use crate::{
    services::{DirectoryEvent, DirectoryRequest, FileSource, LoadHandle, LocationValidationError},
    test_support::gtk_test,
    ui::browser::PeekBehavior,
};

type Validation = Rc<dyn Fn(Result<(), LocationValidationError>)>;

#[derive(Default)]
struct HeldSource {
    validation: RefCell<Option<Validation>>,
}

impl FileSource for HeldSource {
    fn validate_location(&self, _: &Location) -> Result<(), LocationValidationError> {
        Ok(())
    }

    fn validate_location_async(&self, _: Location, emit: Validation) -> LoadHandle {
        self.validation.replace(Some(emit));
        LoadHandle::new(|| {})
    }

    fn enumerate(&self, request: DirectoryRequest, emit: Rc<dyn Fn(DirectoryEvent)>) -> LoadHandle {
        emit(DirectoryEvent::Finished {
            request_id: request.id,
            truncated: false,
            can_trash: None,
            can_delete: None,
        });
        LoadHandle::new(|| {})
    }
}

#[test]
fn remote_tab_titles_wait_for_validation_and_release_on_rejection_or_superseding_navigation() {
    gtk_test(
        "ui::browser::tab_location::tests::remote_tab_titles_wait_for_validation_and_release_on_rejection_or_superseding_navigation",
        || {
            for outcome in ["success", "rejected", "superseded"] {
                let source = Rc::new(HeldSource::default());
                let view = BrowserView::new(source.clone(), PeekBehavior::default());
                let window = gtk::Window::builder().child(&view.widget()).build();
                window.present();
                let parent = Location::uri("sftp://fixture/parent");
                let alpha = Location::uri("sftp://fixture/parent/alpha");
                let beta = Location::uri("sftp://fixture/parent/beta");
                let browser = view.browser();
                browser.navigate(parent.clone());
                browser.descend(0, alpha.clone());
                let complete = source.validation.take().expect("first validation");
                complete(Ok(()));
                let changes = Rc::new(RefCell::new(Vec::new()));
                let observed = changes.clone();
                view.observe_tab_location(move |location| {
                    observed.borrow_mut().push(location.cloned());
                });
                changes.borrow_mut().clear();
                let hold = view.state.hold_tab_location();
                browser.set_active_column(0);
                browser.focus_active();
                hold.navigate(|| browser.descend(0, beta.clone()));
                assert!(
                    changes.borrow().is_empty(),
                    "{outcome}: awaiting validation"
                );
                assert_eq!(browser.active_location(), Some(parent.clone()));
                let complete = source.validation.take().expect("held validation");
                let expected = match outcome {
                    "success" => {
                        complete(Ok(()));
                        beta
                    }
                    "rejected" => {
                        complete(Err(LocationValidationError::Inaccessible));
                        parent
                    }
                    "superseded" => {
                        let replacement = Location::uri("sftp://fixture/replacement");
                        browser.navigate(replacement.clone());
                        complete(Ok(()));
                        replacement
                    }
                    _ => unreachable!(),
                };
                assert_eq!(
                    &*changes.borrow(),
                    &[Some(expected.clone())],
                    "{outcome}: final title"
                );
                assert_eq!(browser.active_location(), Some(expected));
                browser.clear_observer();
                window.destroy();
            }
        },
    );
}
