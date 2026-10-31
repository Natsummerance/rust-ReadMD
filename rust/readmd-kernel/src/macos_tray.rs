//! Native menus share the reader's document, task and recovery services.
use objc2::{
    define_class, msg_send, rc::Retained, runtime::Sel, sel, AnyThread, DefinedClass,
    MainThreadMarker, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSEventModifierFlags as Modifiers, NSImage, NSMenu, NSMenuItem, NSStatusBar,
    NSStatusItem,
};
use objc2_foundation::{NSData, NSObject, NSObjectProtocol, NSSize, NSString};
use std::collections::BTreeMap;

const ACTIONS: &[&str] = &[
    "show",
    "open",
    "exit",
    "save",
    "save-as",
    "undo",
    "redo",
    "new",
    "folder",
    "plugins",
    "pet",
    "settings",
    "close-document",
    "zen",
    "fullscreen",
];
struct Actions {
    send: Box<dyn Fn(&str)>,
}
define_class!(
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = Actions]
    struct ReadMDMenuActions;
    unsafe impl NSObjectProtocol for ReadMDMenuActions {}
    impl ReadMDMenuActions {
        #[unsafe(method(readmdMenuAction:))]
        fn dispatch(&self, sender: &NSMenuItem) {
            if let Ok(index) = usize::try_from(sender.tag()) {
                if let Some(action) = ACTIONS.get(index) { (self.ivars().send)(action); }
            }
        }
    }
);

pub struct Tray {
    bar: Retained<NSStatusBar>,
    item: Retained<NSStatusItem>,
    entries: BTreeMap<&'static str, Vec<Retained<NSMenuItem>>>,
    // Targets are weak in AppKit; retain ours until all menus are released.
    actions: Retained<ReadMDMenuActions>,
    main_menu: Retained<NSMenu>,
    #[cfg(all(feature = "native-qa", debug_assertions))]
    dispatch: BTreeMap<&'static str, (Retained<NSMenu>, isize)>,
}
impl Tray {
    pub fn new(send: impl Fn(&str) + 'static) -> Option<Self> {
        let main = MainThreadMarker::new()?;
        let actions = ReadMDMenuActions::alloc(main).set_ivars(Actions {
            send: Box::new(send),
        });
        let actions: Retained<ReadMDMenuActions> = unsafe { msg_send![super(actions), init] };
        let bar = NSStatusBar::systemStatusBar();
        let item = bar.statusItemWithLength(-1.0);
        let Some(button) = item.button(main) else {
            bar.removeStatusItem(&item);
            return None;
        };
        let data = NSData::with_bytes(include_bytes!("../../../assets/ReadMD-status-template.png"));
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            image.setSize(NSSize::new(18.0, 18.0));
            image.setTemplate(true);
            button.setImage(Some(&image));
        } else {
            button.setTitle(&NSString::from_str("M"));
        }
        button.setToolTip(Some(&NSString::from_str("ReadMD")));
        let main_menu = NSMenu::new(main);
        let mut tray = Self {
            bar,
            item,
            entries: BTreeMap::new(),
            actions,
            main_menu,
            #[cfg(all(feature = "native-qa", debug_assertions))]
            dispatch: BTreeMap::new(),
        };
        let status = NSMenu::new(main);
        for (id, title, key) in [
            ("show", "Open ReadMD", ""),
            ("open", "Open…", ""),
            ("folder", "Open Folder…", ""),
        ] {
            tray.add(&status, id, title, key, None, None);
        }
        status.addItem(&NSMenuItem::separatorItem(main));
        for (id, title) in [
            ("plugins", "Plugins…"),
            ("pet", "Desktop Pet Settings…"),
            ("settings", "Settings…"),
        ] {
            tray.add(&status, id, title, "", None, None);
        }
        status.addItem(&NSMenuItem::separatorItem(main));
        tray.add(&status, "exit", "Quit ReadMD", "", None, None);
        tray.item.setMenu(Some(&status));
        // App / File / Edit order remains stable for real menu-key QA.
        let app = tray.root("app", "ReadMD");
        for (id, title, key) in [
            ("show", "Open ReadMD", ""),
            ("settings", "Settings…", ","),
            ("plugins", "Plugins…", ""),
            ("pet", "Desktop Pet Settings…", ""),
        ] {
            tray.add(&app, id, title, key, None, None);
        }
        app.addItem(&NSMenuItem::separatorItem(main));
        tray.add(&app, "hide", "Hide ReadMD", "h", Some(sel!(hide:)), None);
        app.addItem(&NSMenuItem::separatorItem(main));
        tray.add(&app, "exit", "Quit ReadMD", "q", None, None);
        let file = tray.root("file", "File");
        for (id, title, key) in [
            ("new", "New Document", "n"),
            ("open", "Open…", "o"),
            ("folder", "Open Folder…", ""),
        ] {
            tray.add(&file, id, title, key, None, None);
        }
        file.addItem(&NSMenuItem::separatorItem(main));
        tray.add(&file, "save", "Save", "s", None, None);
        tray.add(
            &file,
            "save-as",
            "Save As…",
            "s",
            None,
            Some(Modifiers::Command | Modifiers::Shift),
        );
        file.addItem(&NSMenuItem::separatorItem(main));
        tray.add(&file, "close-document", "Close", "w", None, None);
        let edit = tray.root("edit", "Edit");
        tray.add(&edit, "undo", "Undo", "z", None, None);
        tray.add(
            &edit,
            "redo",
            "Redo",
            "z",
            None,
            Some(Modifiers::Command | Modifiers::Shift),
        );
        edit.addItem(&NSMenuItem::separatorItem(main));
        for (id, title, key, selector) in [
            ("cut", "Cut", "x", sel!(cut:)),
            ("copy", "Copy", "c", sel!(copy:)),
            ("paste", "Paste", "v", sel!(paste:)),
            ("select-all", "Select All", "a", sel!(selectAll:)),
        ] {
            tray.add(&edit, id, title, key, Some(selector), None);
        }
        let view = tray.root("view", "View");
        tray.add(&view, "zen", "Zen Mode", "", None, None);
        tray.add(
            &view,
            "fullscreen",
            "Fullscreen",
            "f",
            None,
            Some(Modifiers::Command | Modifiers::Control),
        );
        view.addItem(&NSMenuItem::separatorItem(main));
        tray.add(
            &view,
            "minimize",
            "Minimize",
            "m",
            Some(sel!(performMiniaturize:)),
            None,
        );
        NSApplication::sharedApplication(main).setMainMenu(Some(&tray.main_menu));
        Some(tray)
    }
    fn root(&mut self, id: &'static str, title: &str) -> Retained<NSMenu> {
        let main = self.actions.mtm();
        let menu = NSMenu::new(main);
        let root = NSMenuItem::new(main);
        root.setTitle(&NSString::from_str(title));
        root.setSubmenu(Some(&menu));
        self.main_menu.addItem(&root);
        self.entries.entry(id).or_default().push(root);
        menu
    }
    fn add(
        &mut self,
        menu: &Retained<NSMenu>,
        id: &'static str,
        title: &str,
        key: &str,
        native: Option<Sel>,
        modifiers: Option<Modifiers>,
    ) {
        let entry = unsafe {
            NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(self.actions.mtm()),
                &NSString::from_str(title),
                Some(native.unwrap_or(sel!(readmdMenuAction:))),
                &NSString::from_str(key),
            )
        };
        if native.is_none() {
            let tag = ACTIONS
                .iter()
                .position(|action| *action == id)
                .expect("registered native action");
            entry.setTag(tag as isize);
            unsafe {
                entry.setTarget(Some(self.actions.as_ref()));
            }
        }
        if let Some(modifiers) = modifiers {
            entry.setKeyEquivalentModifierMask(modifiers);
        }
        #[cfg(all(feature = "native-qa", debug_assertions))]
        self.dispatch
            .entry(id)
            .or_insert((menu.clone(), menu.numberOfItems()));
        menu.addItem(&entry);
        self.entries.entry(id).or_default().push(entry);
    }
    pub fn available(&self) -> bool {
        self.item.isVisible() && self.item.menu(self.actions.mtm()).is_some()
    }
    pub fn set_labels(&mut self, labels: [&str; 3]) {
        for (id, label) in ["show", "open", "exit"].into_iter().zip(labels) {
            self.label(id, label);
        }
    }
    pub fn set_document_labels(&mut self, labels: [&str; 3]) {
        for (id, label) in ["file", "save", "save-as"].into_iter().zip(labels) {
            self.label(id, label);
        }
    }
    pub fn set_edit_labels(&mut self, labels: [&str; 6]) {
        for (id, label) in ["edit", "undo", "redo", "cut", "copy", "paste"]
            .into_iter()
            .zip(labels)
        {
            self.label(id, label);
        }
    }
    pub fn set_extra_labels(&mut self, labels: &serde_json::Value) {
        if let Some(labels) = labels.as_object() {
            for (id, label) in labels {
                if let Some(label) = label.as_str().filter(|s| s.len() <= 512) {
                    self.label(id, label);
                }
            }
        }
    }
    fn label(&self, id: &str, label: &str) {
        if let Some(entries) = self.entries.get(id) {
            for entry in entries {
                entry.setTitle(&NSString::from_str(label));
            }
        }
    }
    #[cfg(all(feature = "native-qa", debug_assertions))]
    pub fn qa_action(&self, action: &str) -> bool {
        let Some((menu, index)) = self.dispatch.get(action) else {
            return false;
        };
        menu.performActionForItemAtIndex(*index);
        true
    }
    #[cfg(all(feature = "native-qa", debug_assertions))]
    pub fn qa_snapshot(&self) -> serde_json::Value {
        let image = self.item.button(self.actions.mtm()).and_then(|b| b.image());
        serde_json::json!({"template":image.as_ref().is_some_and(|i|i.isTemplate()),"width":image.as_ref().map(|i|i.size().width),"height":image.as_ref().map(|i|i.size().height),"commands":self.dispatch.keys().collect::<Vec<_>>()})
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.item.setMenu(None);
        self.bar.removeStatusItem(&self.item);
        let app = NSApplication::sharedApplication(self.actions.mtm());
        if app.mainMenu().is_some_and(|m| m == self.main_menu) {
            app.setMainMenu(None);
        }
    }
}
