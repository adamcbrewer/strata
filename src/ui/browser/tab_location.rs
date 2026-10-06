// SPDX-License-Identifier: MIT

use std::rc::{Rc, Weak};

use crate::{app::BrowserEvent, model::Location};

use super::{BrowserView, ViewState};

type Observer = Rc<dyn Fn(Option<&Location>)>;

#[derive(Default)]
pub(super) struct TabLocation {
    location: Option<Location>,
    observers: Vec<Observer>,
    next_id: u64,
    pending: Option<Pending>,
}

struct Pending {
    id: u64,
    navigating: bool,
    generation: Option<u64>,
}

pub(super) struct TabLocationHold {
    state: Weak<ViewState>,
    id: u64,
}

impl TabLocationHold {
    pub(super) fn navigate(self, action: impl FnOnce()) {
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let generation = state.browser.navigation_generation();
        {
            let mut location = state.tab_location.borrow_mut();
            let Some(pending) = location
                .pending
                .as_mut()
                .filter(|pending| pending.id == self.id)
            else {
                return;
            };
            // Row teardown must not end a hold while URI validation is still pending.
            pending.navigating = true;
        }
        action();
        let mut location = state.tab_location.borrow_mut();
        if let Some(pending) = location.pending.as_mut()
            && pending.id == self.id
        {
            if state.browser.navigation_generation() != generation {
                pending.generation = Some(state.browser.navigation_generation());
            } else {
                location.pending = None;
            }
        }
        drop(location);
        state.publish_tab_location();
    }
}

impl Drop for TabLocationHold {
    fn drop(&mut self) {
        if let Some(state) = self.state.upgrade() {
            let mut location = state.tab_location.borrow_mut();
            if location
                .pending
                .as_ref()
                .is_some_and(|pending| pending.id == self.id && !pending.navigating)
            {
                location.pending = None;
            }
            drop(location);
            state.publish_tab_location();
        }
    }
}

impl BrowserView {
    pub(in crate::ui) fn observe_tab_location(
        &self,
        observer: impl Fn(Option<&Location>) + 'static,
    ) {
        let observer: Observer = Rc::new(observer);
        let location = self.state.tab_location.borrow().location.clone();
        self.state
            .tab_location
            .borrow_mut()
            .observers
            .push(observer.clone());
        observer(location.as_ref());
    }
}

impl ViewState {
    pub(super) fn hold_tab_location(self: &Rc<Self>) -> TabLocationHold {
        let mut location = self.tab_location.borrow_mut();
        location.next_id += 1;
        let id = location.next_id;
        location.pending = Some(Pending {
            id,
            navigating: false,
            generation: None,
        });
        TabLocationHold {
            state: Rc::downgrade(self),
            id,
        }
    }

    pub(super) fn cancel_tab_location_hold(&self) {
        self.tab_location.borrow_mut().pending = None;
        self.publish_tab_location();
    }

    pub(super) fn refresh_tab_location(&self, event: &BrowserEvent) {
        let finished = matches!(
            event,
            BrowserEvent::Reset
                | BrowserEvent::ColumnAdded { .. }
                | BrowserEvent::ColumnsTruncated { .. }
                | BrowserEvent::ColumnsRelocated { .. }
                | BrowserEvent::NavigationRejected { .. }
                | BrowserEvent::LocationNavigationRejected { .. }
        );
        let mut location = self.tab_location.borrow_mut();
        let superseded = !matches!(event, BrowserEvent::NavigationStarting)
            && location.pending.as_ref().is_some_and(|pending| {
                pending
                    .generation
                    .is_some_and(|generation| generation != self.browser.navigation_generation())
            });
        if finished || superseded {
            location.pending = None;
        }
        drop(location);
        self.publish_tab_location();
    }

    fn publish_tab_location(&self) {
        let current = self.browser.active_location();
        let observers = {
            let mut location = self.tab_location.borrow_mut();
            if location.pending.is_some() || location.location == current {
                return;
            }
            location.location = current.clone();
            location.observers.clone()
        };
        for observer in observers {
            observer(current.as_ref());
        }
    }
}

#[cfg(test)]
mod tests;
