use crate::report;
use crate::scanner::Entry;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortBy {
    Size,
    Name,
    Count,
}

impl SortBy {
    pub fn label(&self) -> &'static str {
        match self {
            SortBy::Size => "size",
            SortBy::Name => "name",
            SortBy::Count => "count",
        }
    }

    pub fn next(&self) -> SortBy {
        match self {
            SortBy::Size => SortBy::Name,
            SortBy::Name => SortBy::Count,
            SortBy::Count => SortBy::Size,
        }
    }
}

pub enum AppAction {
    Continue,
    Quit,
    Rescan { path: PathBuf, came_from: String },
}

pub struct App {
    pub root: Entry,
    pub path_stack: Vec<(usize, usize)>,
    pub current_path: PathBuf,
    pub selected: usize,
    pub scroll_offset: usize,
    pub sort_by: SortBy,
    pub show_help: bool,
    pub show_report: bool,
    pub report_scroll: usize,
    pub confirm_delete: Option<PathBuf>,
    pub message: Option<(String, std::time::Instant)>,
    pub min_bytes: u64,
    pub cached_report: Option<String>,
}

impl App {
    pub fn new(root: Entry, min_bytes: u64) -> Self {
        let current_path = root.path.clone();
        Self {
            root,
            path_stack: Vec::new(),
            current_path,
            selected: 0,
            scroll_offset: 0,
            sort_by: SortBy::Size,
            show_help: false,
            show_report: false,
            report_scroll: 0,
            confirm_delete: None,
            message: None,
            min_bytes,
            cached_report: None,
        }
    }

    pub fn new_with_selection(root: Entry, select_name: &str, min_bytes: u64) -> Self {
        let mut app = Self::new(root, min_bytes);
        let children = app.sorted_children();
        let offset = app.child_offset();
        for (i, child) in children.iter().enumerate() {
            if child.name == select_name {
                app.selected = i + offset;
                break;
            }
        }
        app
    }

    pub fn current_entry(&self) -> &Entry {
        self.find_entry(&self.current_path).unwrap_or(&self.root)
    }

    fn find_entry(&self, path: &PathBuf) -> Option<&Entry> {
        if self.root.path == *path {
            return Some(&self.root);
        }
        Self::find_in_tree(&self.root, path)
    }

    fn find_in_tree<'a>(entry: &'a Entry, path: &PathBuf) -> Option<&'a Entry> {
        for child in &entry.children {
            if child.path == *path {
                return Some(child);
            }
            if path.starts_with(&child.path) {
                if let Some(found) = Self::find_in_tree(child, path) {
                    return Some(found);
                }
            }
        }
        None
    }

    fn has_filesystem_parent(&self) -> bool {
        match self.current_path.parent() {
            Some(parent) => parent != self.current_path && !parent.as_os_str().is_empty(),
            None => false,
        }
    }

    pub fn has_parent(&self) -> bool {
        !self.path_stack.is_empty() || self.has_filesystem_parent()
    }

    fn child_offset(&self) -> usize {
        if self.has_parent() { 1 } else { 0 }
    }

    pub fn display_count(&self) -> usize {
        self.current_entry().children.len() + self.child_offset()
    }

    pub fn sorted_children(&self) -> Vec<&Entry> {
        let entry = self.current_entry();
        let mut kids: Vec<&Entry> = entry.children.iter().collect();
        match self.sort_by {
            SortBy::Size => kids.sort_unstable_by(|a, b| b.size.cmp(&a.size)),
            SortBy::Name => kids.sort_unstable_by(|a, b| {
                a.name
                    .bytes()
                    .map(|c| c.to_ascii_lowercase())
                    .cmp(b.name.bytes().map(|c| c.to_ascii_lowercase()))
            }),
            SortBy::Count => kids.sort_unstable_by(|a, b| b.child_count().cmp(&a.child_count())),
        }
        kids
    }

    pub fn move_down(&mut self) {
        let count = self.display_count();
        if count > 0 && self.selected < count - 1 {
            self.selected += 1;
        }
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn page_down(&mut self, page_size: usize) {
        let count = self.display_count();
        if count == 0 {
            return;
        }
        self.selected = (self.selected + page_size).min(count - 1);
    }

    pub fn page_up(&mut self, page_size: usize) {
        self.selected = self.selected.saturating_sub(page_size);
    }

    pub fn enter_selected(&mut self) -> AppAction {
        if self.has_parent() && self.selected == 0 {
            return self.go_up();
        }

        let child_idx = self.selected - self.child_offset();
        let target = {
            let children = self.sorted_children();
            children
                .get(child_idx)
                .filter(|c| c.is_dir)
                .map(|c| c.path.clone())
        };
        if let Some(path) = target {
            self.path_stack.push((self.selected, self.scroll_offset));
            self.current_path = path;
            self.selected = 1;
            self.scroll_offset = 0;
        }
        AppAction::Continue
    }

    pub fn go_up(&mut self) -> AppAction {
        if !self.path_stack.is_empty() {
            self.go_back();
            AppAction::Continue
        } else if self.has_filesystem_parent() {
            let came_from = self.root.name.clone();
            let parent = self.current_path.parent().unwrap().to_path_buf();
            AppAction::Rescan {
                path: parent,
                came_from,
            }
        } else {
            AppAction::Continue
        }
    }

    fn go_back(&mut self) {
        if let Some((prev_selected, prev_offset)) = self.path_stack.pop() {
            if let Some(parent) = self.current_path.parent() {
                self.current_path = parent.to_path_buf();
                self.selected = prev_selected;
                self.scroll_offset = prev_offset;
            }
        }
    }

    pub fn selected_entry(&self) -> Option<&Entry> {
        if self.has_parent() && self.selected == 0 {
            return None;
        }
        let child_idx = self.selected - self.child_offset();
        let children = self.sorted_children();
        children.get(child_idx).copied()
    }

    pub fn open_report(&mut self) {
        let text = report::generate_report(self.current_entry(), self.min_bytes);
        self.cached_report = Some(text);
        self.show_report = true;
        self.report_scroll = 0;
    }

    pub fn close_report(&mut self) {
        self.show_report = false;
        self.cached_report = None;
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
    }

    pub fn set_message(&mut self, msg: String) {
        self.message = Some((msg, std::time::Instant::now()));
    }

    pub fn tick_message(&mut self) {
        if let Some((_, when)) = &self.message {
            if when.elapsed().as_secs() >= 3 {
                self.message = None;
            }
        }
    }

    pub fn request_delete(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.confirm_delete = Some(entry.path.clone());
        }
    }

    pub fn confirm_delete_yes(&mut self) {
        if let Some(path) = self.confirm_delete.take() {
            let result = if path.is_dir() {
                std::fs::remove_dir_all(&path)
            } else {
                std::fs::remove_file(&path)
            };

            match result {
                Ok(_) => {
                    self.remove_from_tree(&path);
                    let count = self.current_entry().children.len();
                    if self.selected >= count && count > 0 {
                        self.selected = count - 1;
                    }
                    self.set_message(format!("Deleted: {}", path.display()));
                }
                Err(e) => {
                    self.set_message(format!("Error: {e}"));
                }
            }
        }
    }

    pub fn confirm_delete_no(&mut self) {
        self.confirm_delete = None;
    }

    fn remove_from_tree(&mut self, path: &PathBuf) {
        Self::remove_entry(&mut self.root, path);
    }

    fn remove_entry(entry: &mut Entry, path: &PathBuf) -> bool {
        let before_len = entry.children.len();
        entry.children.retain(|c| c.path != *path);

        if entry.children.len() < before_len {
            entry.size = entry.children.iter().map(|c| c.size).sum();
            return true;
        }

        for child in &mut entry.children {
            if path.starts_with(&child.path) && Self::remove_entry(child, path) {
                entry.size = entry.children.iter().map(|c| c.size).sum();
                return true;
            }
        }
        false
    }
}
