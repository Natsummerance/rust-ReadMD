//! Native menu-bar item and application menu. Both enter the same window and
//! recovery actions as the reader; Quit never terminates Cocoa directly.
use objc2::{
    define_class, msg_send, rc::Retained, sel, AnyThread, DefinedClass, MainThreadMarker,
    MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSEventModifierFlags, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
};
use objc2_foundation::{NSData, NSObject, NSObjectProtocol, NSSize, NSString};

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
        #[unsafe(method(showReadMD:))]
        fn show(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("show"); }
        #[unsafe(method(openReadMDFile:))]
        fn open(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("open"); }
        #[unsafe(method(quitReadMD:))]
        fn quit(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("exit"); }
        #[unsafe(method(saveReadMD:))]
        fn save(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("save"); }
        #[unsafe(method(saveReadMDAs:))]
        fn save_as(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("save-as"); }
        #[unsafe(method(undoReadMD:))]
        fn undo(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("undo"); }
        #[unsafe(method(redoReadMD:))]
        fn redo(&self, _sender: Option<&NSMenuItem>) { (self.ivars().send)("redo"); }
    }
);

pub struct Tray {
    bar: Retained<NSStatusBar>,
    item: Retained<NSStatusItem>,
    _menu: Retained<NSMenu>,
    labels: Vec<(usize, Retained<NSMenuItem>)>,
    document_labels: [Retained<NSMenuItem>; 3],
    edit_labels: Vec<Retained<NSMenuItem>>,
    // Menu-item targets are weak. Keep our target alive until both menus go.
    _actions: Retained<ReadMDMenuActions>,
    main_menu: Retained<NSMenu>,
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
        let data = NSData::with_bytes(include_bytes!("../../../assets/ReadMD.png"));
        if let Some(image) = NSImage::initWithData(NSImage::alloc(), &data) {
            image.setSize(NSSize::new(18.0, 18.0));
            button.setImage(Some(&image));
        } else {
            button.setTitle(&NSString::from_str("ReadMD"));
        }
        button.setToolTip(Some(&NSString::from_str("ReadMD")));
        let menu = NSMenu::new(main);
        let application_menu = NSMenu::new(main);
        let main_menu = NSMenu::new(main);
        let application = NSMenuItem::new(main);
        application.setTitle(&NSString::from_str("ReadMD"));
        application.setSubmenu(Some(&application_menu));
        main_menu.addItem(&application);
        let mut labels = Vec::new();
        for (destination, application_only) in [(&menu, false), (&application_menu, true)] {
            for (index, (title, action, key)) in [
                ("ReadMD", sel!(showReadMD:), ""),
                ("Open…", sel!(openReadMDFile:), "o"),
                ("Quit ReadMD", sel!(quitReadMD:), "q"),
            ]
            .into_iter()
            .enumerate()
            {
                if application_only && index == 1 {
                    continue;
                }
                if index == 2 {
                    destination.addItem(&NSMenuItem::separatorItem(main));
                }
                let entry = unsafe {
                    NSMenuItem::initWithTitle_action_keyEquivalent(
                        NSMenuItem::alloc(main),
                        &NSString::from_str(title),
                        Some(action),
                        &NSString::from_str(key),
                    )
                };
                unsafe {
                    entry.setTarget(Some(actions.as_ref()));
                }
                destination.addItem(&entry);
                labels.push((index, entry));
            }
        }
        let file_menu = NSMenu::new(main);
        let file_root = NSMenuItem::new(main);
        file_root.setTitle(&NSString::from_str("File"));
        file_root.setSubmenu(Some(&file_menu));
        main_menu.addItem(&file_root);
        let mut document_entries = Vec::new();
        for (index, (title, selector, key)) in [
            ("Open…", sel!(openReadMDFile:), "o"),
            ("Save", sel!(saveReadMD:), "s"),
            ("Save As…", sel!(saveReadMDAs:), "s"),
        ]
        .into_iter()
        .enumerate()
        {
            if index == 1 {
                file_menu.addItem(&NSMenuItem::separatorItem(main));
            }
            let entry = unsafe {
                NSMenuItem::initWithTitle_action_keyEquivalent(
                    NSMenuItem::alloc(main),
                    &NSString::from_str(title),
                    Some(selector),
                    &NSString::from_str(key),
                )
            };
            unsafe {
                entry.setTarget(Some(actions.as_ref()));
            }
            if index == 2 {
                entry.setKeyEquivalentModifierMask(
                    NSEventModifierFlags::Command | NSEventModifierFlags::Shift,
                );
            }
            file_menu.addItem(&entry);
            if index == 0 {
                labels.push((1, entry));
            } else {
                document_entries.push(entry);
            }
        }
        let document_labels = [
            file_root,
            document_entries.remove(0),
            document_entries.remove(0),
        ];
        let edit_menu = NSMenu::new(main);
        let edit_root = NSMenuItem::new(main);
        edit_root.setTitle(&NSString::from_str("Edit"));
        edit_root.setSubmenu(Some(&edit_menu));
        main_menu.addItem(&edit_root);
        let mut edit_labels = vec![edit_root];
        for (index, (title, selector, key)) in [
            ("Undo", sel!(undoReadMD:), "z"),
            ("Redo", sel!(redoReadMD:), "z"),
            ("Cut", sel!(cut:), "x"),
            ("Copy", sel!(copy:), "c"),
            ("Paste", sel!(paste:), "v"),
        ].into_iter().enumerate() {
            if index == 2 { edit_menu.addItem(&NSMenuItem::separatorItem(main)); }
            let entry = unsafe { NSMenuItem::initWithTitle_action_keyEquivalent(
                NSMenuItem::alloc(main), &NSString::from_str(title), Some(selector), &NSString::from_str(key)) };
            if index < 2 {
                // History belongs to the current editor/document service.
                unsafe { entry.setTarget(Some(actions.as_ref())); }
                if index == 1 { entry.setKeyEquivalentModifierMask(NSEventModifierFlags::Command | NSEventModifierFlags::Shift); }
            }
            // Cut/Copy/Paste use the real native responder chain. No clipboard
            // reads or writes occur merely by constructing the menu.
            edit_menu.addItem(&entry);
            edit_labels.push(entry);
        }
        item.setMenu(Some(&menu));
        NSApplication::sharedApplication(main).setMainMenu(Some(&main_menu));
        Some(Self {
            bar,
            item,
            _menu: menu,
            labels,
            document_labels,
            edit_labels,
            _actions: actions,
            main_menu,
        })
    }
    pub fn available(&self) -> bool {
        self.item.isVisible() && self.item.menu(self._actions.mtm()).is_some()
    }
    pub fn set_labels(&mut self, labels: [&str; 3]) {
        for (index, entry) in &self.labels {
            entry.setTitle(&NSString::from_str(labels[*index]));
        }
    }
    pub fn set_document_labels(&mut self, labels: [&str; 3]) {
        for (entry, label) in self.document_labels.iter().zip(labels) {
            entry.setTitle(&NSString::from_str(label));
        }
    }
    pub fn set_edit_labels(&mut self, labels: [&str; 6]) {
        for (entry, label) in self.edit_labels.iter().zip(labels) {
            entry.setTitle(&NSString::from_str(label));
        }
    }
    #[cfg(all(feature = "native-qa", debug_assertions))]
    pub fn qa_action(&self, action: &str) -> bool {
        let index = match action {
            "show" => 0,
            "open" => 1,
            "exit" => 3,
            _ => return false,
        };
        self._menu.performActionForItemAtIndex(index);
        true
    }
}
impl Drop for Tray {
    fn drop(&mut self) {
        self.item.setMenu(None);
        self.bar.removeStatusItem(&self.item);
        // Release the main menu before its weak targets.
        let app = NSApplication::sharedApplication(self._actions.mtm());
        if app.mainMenu().is_some_and(|menu| menu == self.main_menu) {
            app.setMainMenu(None);
        }
    }
}
