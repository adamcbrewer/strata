// SPDX-License-Identifier: MIT

use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

use gtk::{gio, glib, prelude::*};

use crate::{adapters::gio_file_for_location, model::Location};

use super::{SidebarState, load_pinned_places};

thread_local! {
    static SIDEBARS: RefCell<Vec<Weak<SidebarState>>> = const { RefCell::new(Vec::new()) };
}

pub(super) fn register_sidebar(state: &Rc<SidebarState>) {
    SIDEBARS.with_borrow_mut(|sidebars| {
        sidebars.retain(|sidebar| sidebar.strong_count() > 0);
        sidebars.push(Rc::downgrade(state));
    });
}

pub(super) fn refresh_sidebars() {
    let Ok(places) = load_pinned_places() else {
        return;
    };
    let sidebars: Vec<_> =
        SIDEBARS.with_borrow(|sidebars| sidebars.iter().filter_map(Weak::upgrade).collect());
    for sidebar in sidebars {
        if *sidebar.pinned_places.borrow() != places {
            sidebar.pinned_places.replace(places.clone());
            sidebar.rebuild();
        }
    }
}

pub(super) fn remove_deleted_pins(
    file: &gio::File,
    locations: &[Location],
) -> Result<(), glib::Error> {
    let deleted: Vec<_> = locations.iter().map(gio_file_for_location).collect();
    let mut retries = 0;
    loop {
        let (contents, etag) = match file.load_contents(gio::Cancellable::NONE) {
            Ok(loaded) => loaded,
            Err(error) if error.matches(gio::IOErrorEnum::NotFound) => return Ok(()),
            Err(error) => return Err(error),
        };
        let retained = retain_unrelated_bookmarks(&contents, &deleted);
        if retained == contents.as_ref() {
            return Ok(());
        }
        // Keep other applications' edits, including labels we cannot decode.
        match file.replace_contents(
            &retained,
            etag.as_deref(),
            false,
            gio::FileCreateFlags::NONE,
            gio::Cancellable::NONE,
        ) {
            Ok(_) => return Ok(()),
            Err(error) if retries < 2 && error.matches(gio::IOErrorEnum::WrongEtag) => retries += 1,
            Err(error) => return Err(error),
        }
    }
}

fn retain_unrelated_bookmarks(contents: &[u8], deleted: &[gio::File]) -> Vec<u8> {
    contents
        .split_inclusive(|byte| *byte == b'\n')
        .filter(|line| {
            let uri = line
                .split(|byte| matches!(byte, b' ' | b'\r' | b'\n'))
                .next()
                .unwrap_or_default();
            let Ok(uri) = std::str::from_utf8(uri) else {
                return true;
            };
            let pin = gio::File::for_uri(uri);
            !deleted
                .iter()
                .any(|root| pin.equal(root) || pin.has_prefix(root))
        })
        .flatten()
        .copied()
        .collect()
}

#[cfg(test)]
mod tests;
