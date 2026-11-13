//! Which popup is open: the Dropdown menu, the right-click menu and the Delete dialog.
//!
//! One slot each, and the rules that tie them together live here, not at the call sites:
//! - the right-click menu never outlives the Dropdown menu: closing the Dropdown menu closes it too;
//! - a popup belongs to a Note, and goes with it ([`Popups::dismiss_menus_of`], [`Popups::dismiss_all_of`]);
//! - a change of UI size closes the Dropdown menu and the Delete dialog ([`Popups::dismiss_for_ui_scale`]).
//!
//! Closing a Dropdown menu also means giving its Note's menu button back: the closing calls return the
//! Note the Dropdown menu was open for, and the caller resets that button.

use crate::context_menu::ContextMenu;
use crate::delete_dialog::DeleteDialog;
use crate::dropdown::DropdownMenu;
use crate::platform::NoteId;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub struct Popups {
    dropdown: RefCell<Option<Rc<DropdownMenu>>>,
    context_menu: RefCell<Option<Rc<ContextMenu>>>,
    delete_dialog: RefCell<Option<Rc<DeleteDialog>>>,
}

impl Popups {
    pub fn dropdown(&self) -> Option<Rc<DropdownMenu>> {
        self.dropdown.borrow().clone()
    }

    pub fn context_menu(&self) -> Option<Rc<ContextMenu>> {
        self.context_menu.borrow().clone()
    }

    pub fn delete_dialog(&self) -> Option<Rc<DeleteDialog>> {
        self.delete_dialog.borrow().clone()
    }

    pub fn show_dropdown(&self, menu: DropdownMenu) {
        *self.dropdown.borrow_mut() = Some(Rc::new(menu));
    }

    pub fn show_context_menu(&self, menu: ContextMenu) {
        *self.context_menu.borrow_mut() = Some(Rc::new(menu));
    }

    pub fn show_delete_dialog(&self, dialog: DeleteDialog) {
        *self.delete_dialog.borrow_mut() = Some(Rc::new(dialog));
    }

    pub fn dismiss_context_menu(&self) {
        let menu = self.context_menu.borrow_mut().take();
        if let Some(m) = menu {
            m.close();
        }
    }

    /// Close the Dropdown menu, and the right-click menu with it. Returns the Note the Dropdown menu was
    /// open for, if one was open.
    pub fn dismiss_dropdown(&self) -> Option<NoteId> {
        self.dismiss_context_menu();
        let menu = self.dropdown.borrow_mut().take()?;
        menu.close();
        Some(menu.note_id())
    }

    pub fn dismiss_delete_dialog(&self) {
        let dialog = self.delete_dialog.borrow_mut().take();
        if let Some(d) = dialog {
            d.close();
        }
    }

    /// Close the Delete dialog if it is for Note `id`.
    pub fn dismiss_delete_dialog_of(&self, id: NoteId) {
        if self.delete_dialog().map(|d| d.note_id()) == Some(id) {
            self.dismiss_delete_dialog();
        }
    }

    /// Close the right-click menu and the Dropdown menu if they are for Note `id`. Returns the Note whose
    /// Dropdown menu was closed, if it was.
    pub fn dismiss_menus_of(&self, id: NoteId) -> Option<NoteId> {
        if self.context_menu().map(|m| m.note_id()) == Some(id) {
            self.dismiss_context_menu();
        }
        if self.dropdown().map(|d| d.note_id()) == Some(id) {
            return self.dismiss_dropdown();
        }
        None
    }

    /// Close every popup that is for Note `id`. Returns the Note whose Dropdown menu was closed, if it was.
    pub fn dismiss_all_of(&self, id: NoteId) -> Option<NoteId> {
        let closed = self.dismiss_menus_of(id);
        self.dismiss_delete_dialog_of(id);
        closed
    }

    /// The UI size changed: the popups are placed for the old size. Returns the Note whose Dropdown menu
    /// was closed, if it was.
    pub fn dismiss_for_ui_scale(&self) -> Option<NoteId> {
        let closed = self.dismiss_dropdown();
        self.dismiss_delete_dialog();
        closed
    }
}
