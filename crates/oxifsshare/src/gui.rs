//! The egui front end. All archive logic lives in `oxifsshare-core`; this file
//! only draws state and forwards actions. The app never renders documents and
//! hands files to the operating system's default application.

use std::{
    path::{Path, PathBuf},
    sync::mpsc,
    thread,
};

use eframe::egui::{self, Color32, Key, RichText};
use egui_extras::{Column, TableBuilder};
use oxifsshare_core::{
    emlcache::EmlCache, Archive, ColumnKind, Error, FileKind, FileState, Filter, OpenError, Sort,
};

use crate::{
    picker::{FolderPicker, Hint},
    settings,
    textfmt::{display_text, human_size, parse_date_field, short_datetime},
};

const KINDS: [(FileKind, &str); 6] = [
    (FileKind::Pdf, "PDF"),
    (FileKind::Tiff, "TIFF"),
    (FileKind::Jpeg, "JPG"),
    (FileKind::Msg, "MSG"),
    (FileKind::Other, "Other"),
    (FileKind::Missing, "Missing"),
];
const ROW_HEIGHT: f32 = 20.0;
const CELL_CHARS: usize = 200;

/// Starts a document with the operating system. Replaceable in tests.
pub type Opener = Box<dyn Fn(&Path) -> Result<(), String>>;

fn system_opener() -> Opener {
    Box::new(|p| open::that_detached(p).map_err(|e| e.to_string()))
}

/// Opens the window and runs until it is closed.
pub fn run(initial: Option<PathBuf>) -> Result<(), eframe::Error> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([700.0, 420.0])
            .with_title("OxiFSShare")
            .with_app_id("oxifsshare"),
        ..Default::default()
    };
    eframe::run_native(
        "oxifsshare",
        options,
        Box::new(move |_cc| Ok(Box::new(App::new(initial.clone(), system_opener())))),
    )
}

/// All UI state.
pub struct App {
    path_input: String,
    archive: Option<Archive>,
    generation: u64,
    loading: Option<mpsc::Receiver<Result<Archive, Error>>>,
    error: Option<String>,
    notice: Option<String>,
    text: String,
    doc_type: String,
    doc_types: Vec<String>,
    kind_on: [bool; 6],
    date_from: String,
    date_to: String,
    sort: Option<Sort>,
    rows: Vec<usize>,
    last_query: Option<(Filter, Option<Sort>, bool)>,
    selected: Option<usize>,
    scroll_to: Option<usize>,
    confirm: Option<usize>,
    focus_search: bool,
    opener: Opener,
    eml: EmlCache,
    picker: Option<FolderPicker>,
    pending_autoload: bool,
    /// Save the path after a successful load (disabled in tests).
    remember_path: bool,
}

enum Action {
    Select(usize),
    Open(usize),
    SortBy(usize),
}

impl App {
    /// Creates the app. With a path, loading starts on the first frame. Without one the
    /// window starts empty: the last-used path is only filled into the text box (and
    /// offered by "Browse…"), never loaded on its own.
    pub fn new(initial: Option<PathBuf>, opener: Opener) -> App {
        let (path_input, autoload) = match initial {
            Some(p) => (p.to_string_lossy().into_owned(), true),
            None => (settings::load_last_path().unwrap_or_default(), false),
        };
        App {
            path_input,
            archive: None,
            generation: 0,
            loading: None,
            error: None,
            notice: None,
            text: String::new(),
            doc_type: String::new(),
            doc_types: Vec::new(),
            kind_on: [true; 6],
            date_from: String::new(),
            date_to: String::new(),
            sort: None,
            rows: Vec::new(),
            last_query: None,
            selected: None,
            scroll_to: None,
            confirm: None,
            focus_search: false,
            opener,
            eml: EmlCache::new(),
            picker: None,
            pending_autoload: autoload,
            remember_path: true,
        }
    }

    /// Starts loading `path_input` on a background thread.
    pub fn start_load(&mut self, ctx: &egui::Context) {
        let path = PathBuf::from(self.path_input.trim());
        if self.path_input.trim().is_empty() {
            self.error = Some("Enter the path of the archive folder first.".into());
            return;
        }
        let (tx, rx) = mpsc::channel();
        let ctx = ctx.clone();
        thread::spawn(move || {
            let _ = tx.send(Archive::open(&path));
            ctx.request_repaint();
        });
        self.loading = Some(rx);
        self.error = None;
        self.notice = None;
    }

    fn poll_load(&mut self) {
        let Some(rx) = &self.loading else { return };
        match rx.try_recv() {
            Ok(Ok(archive)) => {
                self.loading = None;
                if self.remember_path {
                    settings::save_last_path(self.path_input.trim());
                }
                self.install(archive);
            }
            Ok(Err(e)) => {
                self.loading = None;
                self.error = Some(e.to_string());
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.loading = None;
                self.error = Some("Loading stopped unexpectedly.".into());
            }
        }
    }

    /// Replaces the shown archive.
    pub fn install(&mut self, archive: Archive) {
        self.doc_types = archive.doc_type_values();
        if !self.doc_types.contains(&self.doc_type) {
            self.doc_type.clear();
        }
        self.sort = None;
        self.selected = None;
        self.scroll_to = None;
        self.last_query = None;
        self.generation += 1;
        self.archive = Some(archive);
        self.error = None;
    }

    fn current_filter(&self) -> (Filter, bool) {
        let from = parse_date_field(&self.date_from);
        let to = parse_date_field(&self.date_to);
        let all_on = self.kind_on.iter().all(|&k| k);
        let kinds = if all_on {
            Vec::new()
        } else {
            KINDS
                .iter()
                .zip(self.kind_on)
                .filter(|(_, on)| *on)
                .map(|((k, _), _)| *k)
                .collect()
        };
        let f = Filter {
            text: self.text.clone(),
            doc_type: (!self.doc_type.is_empty()).then(|| self.doc_type.clone()),
            kinds,
            date_from: from.unwrap_or(None),
            date_to: to.unwrap_or(None),
        };
        (f, self.kind_on.iter().any(|&k| k))
    }

    /// Recomputes the visible rows when the query changed.
    pub fn refresh(&mut self) {
        let Some(a) = &self.archive else { return };
        let (f, any_kind) = self.current_filter();
        let key = (f, self.sort, any_kind);
        if self.last_query.as_ref() == Some(&key) {
            return;
        }
        self.rows = if any_kind {
            a.query(&key.0, key.1)
        } else {
            Vec::new()
        };
        self.last_query = Some(key);
    }

    /// Opens a row's document through the OS. Outlook messages are converted to
    /// `.eml` in a private temporary folder first; if that fails the original is not
    /// opened silently, the error says why and "Open original .msg" is offered.
    pub fn open_row(&mut self, row: usize, confirmed: bool) {
        let Some(a) = &self.archive else { return };
        match a.open_target(row, confirmed) {
            Ok(p) => {
                let is_msg = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("msg"));
                if is_msg {
                    self.open_converted(&p);
                } else {
                    self.hand_over(&p);
                }
            }
            Err(OpenError::NeedsConfirmation(_)) => self.confirm = Some(row),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    /// Opens the `.msg` file itself with the OS, without converting.
    pub fn open_original_row(&mut self, row: usize) {
        let Some(a) = &self.archive else { return };
        match a.open_target(row, false) {
            Ok(p) => self.hand_over(&p),
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn open_converted(&mut self, msg_path: &Path) {
        match self.eml.convert(msg_path) {
            Ok((eml, _)) => match (self.opener)(&eml) {
                Ok(()) => {
                    self.notice = Some(
                        "Converted the message to .eml and handed it to your default mail application."
                            .into(),
                    )
                }
                Err(e) => {
                    self.error = Some(format!(
                        "Could not start an application for the converted message ({e}). Try \"Open original .msg\"."
                    ))
                }
            },
            Err(e) => {
                self.error = Some(format!(
                    "Could not convert this message: {e}. You can try \"Open original .msg\" to let your system handle it."
                ))
            }
        }
    }

    fn hand_over(&mut self, p: &Path) {
        match (self.opener)(p) {
            Ok(()) => self.notice = Some("Handed the file to your default application.".into()),
            Err(e) => {
                self.error = Some(format!(
                    "Could not start your default application ({e}). Is xdg-utils installed? You can still use Copy path."
                ))
            }
        }
    }

    fn reveal_row(&mut self, row: usize) {
        let Some(a) = &self.archive else { return };
        match a.open_target(row, true) {
            Ok(p) => {
                let dir = p
                    .parent()
                    .map_or_else(|| a.location.folder.clone(), Path::to_path_buf);
                if let Err(e) = (self.opener)(&dir) {
                    self.error = Some(format!(
                        "Could not open the folder in your file manager ({e})."
                    ));
                } else {
                    self.notice = Some("Opened the containing folder.".into());
                }
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }

    fn move_selection(&mut self, delta: isize) {
        if self.rows.is_empty() {
            return;
        }
        let pos = self
            .selected
            .and_then(|s| self.rows.iter().position(|&r| r == s));
        let next = match pos {
            None => 0,
            Some(p) => (p as isize + delta).clamp(0, self.rows.len() as isize - 1) as usize,
        };
        self.selected = Some(self.rows[next]);
        self.scroll_to = Some(next);
    }

    /// Draws one frame. Separate from `eframe::App::update` so tests can run it headless.
    /// Ctrl + mouse wheel (or touchpad pinch) scales the whole interface.
    /// Ctrl + plus/minus/0 are handled by egui itself. Not remembered between runs.
    fn apply_zoom(ctx: &egui::Context) {
        let delta = ctx.input(|i| i.zoom_delta());
        if (delta - 1.0).abs() > f32::EPSILON {
            let z = (ctx.zoom_factor() * delta).clamp(0.5, 4.0);
            ctx.set_zoom_factor(z);
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        Self::apply_zoom(ctx);
        if std::mem::take(&mut self.pending_autoload) {
            self.start_load(ctx);
        }
        self.poll_load();
        self.refresh();
        self.handle_keys(ctx);

        egui::TopBottomPanel::top("top").show(ctx, |ui| self.top_bar(ui, ctx));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));
        if self.archive.is_some() {
            egui::SidePanel::right("details")
                .resizable(true)
                .default_width(460.0)
                .min_width(260.0)
                .show(ctx, |ui| self.details(ui, ctx));
        }
        egui::CentralPanel::default().show(ctx, |ui| self.table(ui));
        self.confirm_dialog(ctx);
        self.picker_window(ctx);
    }

    fn handle_keys(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, Key::F)) {
            self.focus_search = true;
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let (down, up, enter) = ctx.input(|i| {
            (
                i.key_pressed(Key::ArrowDown),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::Enter),
            )
        });
        if down {
            self.move_selection(1);
        }
        if up {
            self.move_selection(-1);
        }
        if enter {
            if let Some(s) = self.selected {
                self.open_row(s, false);
            }
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label("Archive:");
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.path_input)
                    .hint_text("folder with fs-share.xml (or its parent)")
                    .desired_width(ui.available_width() - 260.0),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let busy = self.loading.is_some();
            if ui
                .add_enabled(!busy, egui::Button::new("Browse…"))
                .on_hover_text("Choose the archive folder")
                .clicked()
            {
                let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                    .map(PathBuf::from);
                self.picker = Some(FolderPicker::new(Path::new(self.path_input.trim()), home));
            }
            if ui
                .add_enabled(!busy, egui::Button::new("Open archive"))
                .clicked()
                || (enter && !busy)
            {
                self.start_load(ctx);
            }
            if busy {
                ui.spinner();
            }
        });
        if let Some(e) = self.error.clone() {
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(Color32::from_rgb(220, 70, 70), format!("⚠ {e}"));
                if ui.small_button("dismiss").clicked() {
                    self.error = None;
                }
            });
        }
        if self.archive.is_some() {
            ui.horizontal_wrapped(|ui| {
                let search_id = egui::Id::new("search_box");
                if self.focus_search {
                    ui.memory_mut(|m| m.request_focus(search_id));
                    self.focus_search = false;
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.text)
                        .id(search_id)
                        .hint_text("search all columns (Ctrl+F); several words = AND")
                        .desired_width(300.0),
                );
                if ui.button("Clear").clicked() {
                    self.text.clear();
                    self.doc_type.clear();
                    self.kind_on = [true; 6];
                    self.date_from.clear();
                    self.date_to.clear();
                }
                if !self.doc_types.is_empty() {
                    let label = if self.doc_type.is_empty() {
                        "Any type".to_string()
                    } else {
                        self.doc_type.clone()
                    };
                    egui::ComboBox::from_id_salt("doc_type")
                        .selected_text(label)
                        .width(160.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.doc_type, String::new(), "Any type");
                            for t in &self.doc_types {
                                ui.selectable_value(
                                    &mut self.doc_type,
                                    t.clone(),
                                    display_text(t, 60),
                                );
                            }
                        });
                }
                ui.separator();
                for (i, (_, label)) in KINDS.iter().enumerate() {
                    ui.checkbox(&mut self.kind_on[i], *label);
                }
                ui.separator();
                self.date_edit(ui, true);
                self.date_edit(ui, false);
            });
        }
        ui.add_space(4.0);
    }

    fn date_edit(&mut self, ui: &mut egui::Ui, from: bool) {
        let (label, buf) = if from {
            ("From", &mut self.date_from)
        } else {
            ("To", &mut self.date_to)
        };
        ui.label(label);
        let bad = parse_date_field(buf).is_err();
        let mut edit = egui::TextEdit::singleline(buf)
            .hint_text("YYYY-MM-DD")
            .desired_width(86.0);
        if bad {
            edit = edit.text_color(Color32::from_rgb(220, 70, 70));
        }
        let r = ui.add(edit);
        if bad {
            r.on_hover_text("Use YYYY-MM-DD, or leave empty. Ignored until valid.");
        }
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| match &self.archive {
            None => {
                ui.label(if self.loading.is_some() {
                    "Loading…"
                } else {
                    "No archive open"
                });
            }
            Some(a) => {
                let s = a.stats();
                ui.strong(format!("{} of {} entries", self.rows.len(), s.entries));
                ui.separator();
                ui.label(format!("files matched {}", s.matched));
                ui.label(format!("missing {}", s.missing));
                ui.label(format!("orphan files {}", s.orphans));
                if s.ambiguous > 0 {
                    ui.label(format!("ambiguous {}", s.ambiguous));
                }
                if let Some(c) = a.key.chosen {
                    ui.separator();
                    ui.weak(format!("key column: {}", a.table.columns[c].name));
                }
                if let Some(n) = self.notice.as_deref() {
                    ui.separator();
                    ui.label(n);
                }
            }
        });
    }

    fn table(&mut self, ui: &mut egui::Ui) {
        let Some(archive) = &self.archive else {
            ui.centered_and_justified(|ui| {
                ui.label("Enter the archive folder above and press Open archive.");
            });
            return;
        };
        let cols = &archive.table.columns;
        let mut action: Option<Action> = None;
        let selected = self.selected;
        let scroll_to = self.scroll_to.take();
        let sort = self.sort;
        let rows = &self.rows;

        ui.push_id(self.generation, |ui| {
            egui::ScrollArea::horizontal()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let avail_h = ui.available_height();
                    let mut tb = TableBuilder::new(ui)
                        .striped(true)
                        .resizable(true)
                        .sense(egui::Sense::click())
                        .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
                        .min_scrolled_height(0.0)
                        .max_scroll_height(avail_h)
                        .column(Column::exact(64.0));
                    for c in cols {
                        let lname = c.name.to_lowercase();
                        let w = match c.kind {
                            ColumnKind::DateTime => 125.0,
                            ColumnKind::Int | ColumnKind::Number => 90.0,
                            ColumnKind::Text
                                if ["note", "address", "description", "name"]
                                    .iter()
                                    .any(|k| lname.contains(k)) =>
                            {
                                240.0
                            }
                            ColumnKind::Text => 140.0,
                        };
                        tb = tb.column(Column::initial(w).at_least(40.0).clip(true));
                    }
                    if let Some(pos) = scroll_to {
                        tb = tb.scroll_to_row(pos, None);
                    }
                    tb.header(24.0, |mut header| {
                        header.col(|ui| {
                            ui.strong("File");
                        });
                        for (i, c) in cols.iter().enumerate() {
                            header.col(|ui| {
                                let r = ui.add(
                                    egui::Label::new(RichText::new(&c.label).strong())
                                        .truncate()
                                        .sense(egui::Sense::click()),
                                );
                                if let Some(s) = sort.filter(|s| s.column == i) {
                                    // Drawn as a shape: the default fonts have no triangle glyphs.
                                    let p = egui::pos2(r.rect.right() - 10.0, r.rect.center().y);
                                    let (h, w) = (3.5, 4.0);
                                    let pts = if s.descending {
                                        vec![
                                            egui::pos2(p.x - w, p.y - h),
                                            egui::pos2(p.x + w, p.y - h),
                                            egui::pos2(p.x, p.y + h),
                                        ]
                                    } else {
                                        vec![
                                            egui::pos2(p.x - w, p.y + h),
                                            egui::pos2(p.x + w, p.y + h),
                                            egui::pos2(p.x, p.y - h),
                                        ]
                                    };
                                    ui.painter().add(egui::Shape::convex_polygon(
                                        pts,
                                        ui.visuals().strong_text_color(),
                                        egui::Stroke::NONE,
                                    ));
                                }
                                if r.on_hover_text("Click to sort").clicked() {
                                    action = Some(Action::SortBy(i));
                                }
                            });
                        }
                    })
                    .body(|body| {
                        body.rows(ROW_HEIGHT, rows.len(), |mut row| {
                            let ri = rows[row.index()];
                            row.set_selected(selected == Some(ri));
                            row.col(|ui| kind_cell(ui, archive, ri));
                            for (ci, col) in cols.iter().enumerate() {
                                row.col(|ui| {
                                    let v = archive.table.rows[ri].get(ci).unwrap_or("");
                                    let shown = if col.kind == ColumnKind::DateTime {
                                        short_datetime(v)
                                    } else {
                                        display_text(v, CELL_CHARS)
                                    };
                                    ui.add(egui::Label::new(shown).truncate().selectable(false));
                                });
                            }
                            let resp = row.response();
                            if resp.double_clicked() {
                                action = Some(Action::Open(ri));
                            } else if resp.clicked() {
                                action = Some(Action::Select(ri));
                            }
                        });
                    });
                });
        });

        match action {
            Some(Action::Select(r)) => self.selected = Some(r),
            Some(Action::Open(r)) => {
                self.selected = Some(r);
                self.open_row(r, false);
            }
            Some(Action::SortBy(c)) => {
                self.sort = match self.sort {
                    Some(s) if s.column == c && !s.descending => Some(Sort {
                        column: c,
                        descending: true,
                    }),
                    Some(s) if s.column == c => None,
                    _ => Some(Sort {
                        column: c,
                        descending: false,
                    }),
                };
            }
            None => {}
        }
    }

    fn details(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(sel) = self.selected else {
            ui.add_space(8.0);
            ui.weak("Select an entry to see its details.");
            return;
        };
        let Some(archive) = &self.archive else { return };
        let file = archive.row_file(sel).cloned();
        let res = archive.resolution(sel);
        let kind = archive.row_kind(sel);
        let mut do_open = false;
        let mut do_reveal = false;
        let mut do_original = false;
        let mut copy: Option<String> = None;

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .add_enabled(file.is_some(), egui::Button::new("Open"))
                .on_hover_text("Open with the default application (Enter)")
                .clicked()
            {
                do_open = true;
            }
            if ui
                .add_enabled(file.is_some(), egui::Button::new("Reveal"))
                .on_hover_text("Open the containing folder in your file manager")
                .clicked()
            {
                do_reveal = true;
            }
            if kind == FileKind::Msg
                && ui
                    .button("Open original .msg")
                    .on_hover_text(
                        "Skip the conversion and let your system open the .msg file itself",
                    )
                    .clicked()
            {
                do_original = true;
            }
            if ui
                .add_enabled(file.is_some(), egui::Button::new("Copy path"))
                .clicked()
            {
                copy = file.as_ref().map(|f| f.path.display().to_string());
            }
        });
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.strong("File");
            egui::Grid::new("file_grid").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                ui.label("Kind");
                match kind {
                    FileKind::Missing => ui.colored_label(Color32::from_rgb(220, 70, 70), "document not found"),
                    FileKind::Msg => ui.colored_label(Color32::from_rgb(230, 150, 40), "Outlook message (.msg), converted to .eml when opened"),
                    k => ui.label(k.label()),
                };
                ui.end_row();
                if let Some(f) = &file {
                    ui.label("Path");
                    ui.add(egui::Label::new(f.path.display().to_string()).wrap().selectable(true));
                    ui.end_row();
                    ui.label("Size");
                    ui.label(human_size(f.size));
                    ui.end_row();
                    if res.candidates > 1 {
                        ui.label("Note");
                        ui.label(format!("{} files share this key; the first by type and name was chosen", res.candidates));
                        ui.end_row();
                    }
                    if archive.file_state(res.file.unwrap_or(0)) == FileState::Orphan {
                        ui.label("Note");
                        ui.label("no entry refers to this file");
                        ui.end_row();
                    }
                }
            });
            ui.add_space(8.0);
            ui.strong("Fields");
            egui::Grid::new("fields_grid").num_columns(2).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
                for (ci, c) in archive.table.columns.iter().enumerate() {
                    ui.label(RichText::new(&c.label).weak());
                    match archive.table.rows[sel].get(ci) {
                        Some(v) if !v.is_empty() => {
                            ui.add(egui::Label::new(v).wrap().selectable(true));
                        }
                        Some(_) => {
                            ui.weak("(empty)");
                        }
                        None => {
                            ui.weak("—");
                        }
                    }
                    ui.end_row();
                }
            });
        });

        if do_open {
            self.open_row(sel, false);
        }
        if do_original {
            self.open_original_row(sel);
        }
        if do_reveal {
            self.reveal_row(sel);
        }
        if let Some(p) = copy {
            ctx.copy_text(p);
            self.notice = Some("Path copied to the clipboard.".into());
        }
    }

    fn picker_window(&mut self, ctx: &egui::Context) {
        let Some(picker) = &mut self.picker else {
            return;
        };
        let mut choose: Option<PathBuf> = None;
        let mut close = false;
        egui::Window::new("Choose the archive folder")
            .collapsible(false)
            .default_size([560.0, 460.0])
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("⬆ Up").clicked() {
                        picker.up();
                    }
                    ui.add(
                        egui::Label::new(
                            RichText::new(picker.dir().display().to_string()).strong(),
                        )
                        .truncate(),
                    );
                });
                if let Some(e) = picker.error() {
                    ui.colored_label(Color32::from_rgb(220, 70, 70), format!("⚠ {e}"));
                }
                match picker.hint() {
                    Hint::Archive => {
                        ui.colored_label(
                            Color32::from_rgb(70, 170, 90),
                            "✔ fs-share.xml is in this folder",
                        );
                    }
                    Hint::ArchiveInSubfolder => {
                        ui.colored_label(
                            Color32::from_rgb(70, 170, 90),
                            "✔ an fsshare folder with fs-share.xml is inside",
                        );
                    }
                    Hint::None => {
                        ui.weak("No fs-share.xml here. Open folders until you see the ✔ mark.");
                    }
                }
                let mut hidden = picker.show_hidden;
                if ui.checkbox(&mut hidden, "show hidden folders").changed() {
                    picker.show_hidden = hidden;
                    picker.refresh();
                }
                ui.separator();
                let mut enter: Option<String> = None;
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 40.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for name in picker.folders() {
                            if ui.selectable_label(false, format!("📁 {name}")).clicked() {
                                enter = Some(name.clone());
                            }
                        }
                        if picker.folders().is_empty() {
                            ui.weak("(no sub-folders)");
                        }
                    });
                if let Some(n) = enter {
                    picker.enter(&n);
                }
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button("Use this folder").clicked() {
                        choose = Some(picker.dir().to_path_buf());
                    }
                    if ui.button("Cancel").clicked() {
                        close = true;
                    }
                });
            });
        if close || ctx.input(|i| i.key_pressed(Key::Escape)) {
            self.picker = None;
        }
        if let Some(p) = choose {
            self.picker = None;
            self.path_input = p.to_string_lossy().into_owned();
            self.start_load(ctx);
        }
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(row) = self.confirm else { return };
        let mut close = false;
        let mut go = false;
        egui::Window::new("Open this file?").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label("This file type is not a usual document type. oxifsshare never runs files itself,");
            ui.label("but your system may start a program for it. Open it anyway?");
            ui.horizontal(|ui| {
                if ui.button("Open anyway").clicked() {
                    go = true;
                }
                if ui.button("Cancel").clicked() {
                    close = true;
                }
            });
        });
        if go {
            self.confirm = None;
            self.open_row(row, true);
        } else if close {
            self.confirm = None;
        }
    }
}

fn kind_cell(ui: &mut egui::Ui, a: &Archive, row: usize) {
    match a.row_kind(row) {
        FileKind::Missing => {
            ui.label(RichText::new("missing").color(Color32::from_rgb(220, 70, 70)));
        }
        FileKind::Msg => {
            ui.label(RichText::new("MSG ✉").color(Color32::from_rgb(230, 150, 40)))
                .on_hover_text("Outlook message. Open converts it to .eml (in a private temporary folder) and hands that to your mail application.");
        }
        k => {
            ui.label(k.label());
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::RefCell,
        rc::Rc,
        time::{Duration, Instant},
    };

    use oxifsshare_core::testgen::{synth_archive, Spec, Variant};

    use super::*;

    type Calls = Rc<RefCell<Vec<PathBuf>>>;

    fn recorder() -> (Opener, Calls) {
        let calls: Calls = Rc::new(RefCell::new(Vec::new()));
        let c = calls.clone();
        (
            Box::new(move |p| {
                c.borrow_mut().push(p.to_path_buf());
                Ok(())
            }),
            calls,
        )
    }

    fn frame(app: &mut App, ctx: &egui::Context) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 760.0),
            )),
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| app.ui(ctx));
    }

    fn loaded_app(spec: &Spec) -> (App, egui::Context, Calls, oxifsshare_core::testgen::Fixture) {
        let fx = synth_archive(spec).unwrap();
        let (op, calls) = recorder();
        let mut app = App::new(Some(fx.root.clone()), op);
        app.remember_path = false;
        let ctx = egui::Context::default();
        let t0 = Instant::now();
        while app.archive.is_none() {
            frame(&mut app, &ctx);
            assert!(
                t0.elapsed() < Duration::from_secs(60),
                "load timed out; error: {:?}",
                app.error
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        frame(&mut app, &ctx);
        (app, ctx, calls, fx)
    }

    #[test]
    fn loads_filters_sorts_and_opens() {
        let spec = Spec {
            rows: 300,
            missing_every: 10,
            variant: Variant::Financial,
            ..Spec::default()
        };
        let (mut app, ctx, calls, fx) = loaded_app(&spec);
        assert_eq!(app.rows.len(), 300);

        app.text = "MINUTES".into();
        frame(&mut app, &ctx);
        assert!(!app.rows.is_empty() && app.rows.len() < 300);
        app.text.clear();

        app.kind_on = [true, true, true, true, true, false];
        frame(&mut app, &ctx);
        assert_eq!(app.rows.len(), 300 - fx.expected.missing, "hiding Missing");
        app.kind_on = [false; 6];
        frame(&mut app, &ctx);
        assert!(app.rows.is_empty(), "no kinds ticked shows nothing");
        app.kind_on = [true; 6];

        app.date_from = "2012-01-01".into();
        app.date_to = "2012-12-31".into();
        frame(&mut app, &ctx);
        let in_2012 = app.rows.len();
        assert!(in_2012 > 0 && in_2012 < 300);
        app.date_from = "not a date".into();
        frame(&mut app, &ctx);
        assert!(
            app.rows.len() >= in_2012,
            "an invalid date is ignored, not applied"
        );
        app.date_from.clear();
        app.date_to.clear();

        let id_col = app
            .archive
            .as_ref()
            .unwrap()
            .table
            .find_column("Document ID")
            .unwrap();
        app.sort = Some(Sort {
            column: id_col,
            descending: true,
        });
        frame(&mut app, &ctx);
        assert_eq!(app.rows.len(), 300);
        assert!(app.rows[0] > app.rows[1], "descending by id");

        // Keyboard-free open path: select a row that has a file and open it.
        let with_file = *app
            .rows
            .iter()
            .find(|&&r| {
                let a = app.archive.as_ref().unwrap();
                a.row_file(r).is_some() && a.row_kind(r) != FileKind::Msg
            })
            .unwrap();
        app.selected = Some(with_file);
        frame(&mut app, &ctx);
        app.open_row(with_file, false);
        assert_eq!(calls.borrow().len(), 1);
        assert!(calls.borrow()[0].starts_with(std::fs::canonicalize(fx.dir.path()).unwrap()));
        assert!(app.error.is_none());

        // Reveal hands over the containing folder.
        app.reveal_row(with_file);
        assert!(calls.borrow()[1].is_dir());

        // A row without a file is a clear error, not an open.
        let missing = fx.expected.missing_ids[0] - 100_000;
        app.open_row(missing as usize, false);
        assert_eq!(calls.borrow().len(), 2);
        assert!(app.error.as_deref().unwrap().contains("not found"));
    }

    #[test]
    fn keyboard_selection_moves_within_visible_rows() {
        let (mut app, ctx, _calls, _fx) = loaded_app(&Spec {
            rows: 20,
            ..Spec::default()
        });
        app.move_selection(1);
        assert_eq!(app.selected, Some(app.rows[0]));
        app.move_selection(1);
        assert_eq!(app.selected, Some(app.rows[1]));
        app.move_selection(-5);
        assert_eq!(app.selected, Some(app.rows[0]));
        app.move_selection(1000);
        assert_eq!(app.selected, Some(*app.rows.last().unwrap()));
        frame(&mut app, &ctx);
    }

    #[test]
    fn unusual_extension_needs_confirmation_and_failed_opener_is_reported() {
        let d = oxifsshare_core::testgen::TempDir::new("gui-conf").unwrap();
        std::fs::write(
            d.path().join("fs-share.xml"),
            "<NewDataSet><documents><Document_x0020_ID>1</Document_x0020_ID></documents></NewDataSet>",
        )
        .unwrap();
        std::fs::write(d.path().join("1.sh"), b"x").unwrap();
        let failing: Opener = Box::new(|_| Err("no opener installed".into()));
        let mut app = App::new(None, failing);
        app.remember_path = false;
        app.install(Archive::open(d.path()).unwrap());
        let ctx = egui::Context::default();
        frame(&mut app, &ctx);
        app.open_row(0, false);
        assert_eq!(app.confirm, Some(0), "asks first");
        frame(&mut app, &ctx);
        app.open_row(0, true);
        let e = app.error.clone().unwrap();
        assert!(e.contains("xdg-utils") && e.contains("Copy path"), "{e}");
    }

    #[test]
    fn bad_path_and_malformed_index_do_not_crash_the_app() {
        let (op, _) = recorder();
        let mut app = App::new(Some(PathBuf::from("/definitely/not/here")), op);
        app.remember_path = false;
        let ctx = egui::Context::default();
        let t0 = Instant::now();
        while app.error.is_none() {
            frame(&mut app, &ctx);
            assert!(t0.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.archive.is_none());

        let d = oxifsshare_core::testgen::TempDir::new("gui-bad").unwrap();
        std::fs::write(
            d.path().join("fs-share.xml"),
            "<NewDataSet>\n<documents>\n<A>x</B>",
        )
        .unwrap();
        app.path_input = d.path().to_string_lossy().into_owned();
        app.error = None;
        app.start_load(&ctx);
        let t0 = Instant::now();
        while app.error.is_none() {
            frame(&mut app, &ctx);
            assert!(t0.elapsed() < Duration::from_secs(10));
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(
            app.error.as_deref().unwrap().contains("line 3"),
            "{:?}",
            app.error
        );
    }

    #[test]
    fn a_hundred_thousand_rows_render_and_requery() {
        let spec = Spec {
            rows: 100_000,
            no_files: true,
            variant: Variant::Financial,
            ..Spec::default()
        };
        let fx = synth_archive(&spec).unwrap();
        let (op, _) = recorder();
        let mut app = App::new(None, op);
        app.remember_path = false;
        app.install(Archive::open(&fx.root).unwrap());
        let ctx = egui::Context::default();
        frame(&mut app, &ctx);
        assert_eq!(app.rows.len(), 100_000);
        app.text = "invoice".into();
        let t = Instant::now();
        for _ in 0..3 {
            frame(&mut app, &ctx);
        }
        eprintln!(
            "3 frames over 100k rows incl. one re-query: {:?}",
            t.elapsed()
        );
    }

    #[test]
    fn msg_rows_are_converted_to_eml_and_originals_stay_available() {
        let spec = Spec {
            rows: 40,
            missing_every: 0,
            variant: Variant::Financial,
            ..Spec::default()
        };
        let (mut app, ctx, calls, _fx) = loaded_app(&spec);
        let msg_row = (0..40)
            .find(|&r| app.archive.as_ref().unwrap().row_kind(r) == FileKind::Msg)
            .expect("fixture has msg rows");
        app.selected = Some(msg_row);
        frame(&mut app, &ctx);
        app.open_row(msg_row, false);
        let eml = calls.borrow()[0].clone();
        assert_eq!(eml.extension().unwrap(), "eml");
        let text = std::fs::read_to_string(&eml).unwrap();
        assert!(text.contains("Subject: Synthetic message"));
        assert!(app.error.is_none());
        // The original is still reachable.
        app.open_original_row(msg_row);
        let orig = calls.borrow()[1].clone();
        assert!(orig.extension().unwrap().eq_ignore_ascii_case("msg"));
        // Temporary files go away with the app.
        let dir = eml.parent().unwrap().to_path_buf();
        drop(app);
        assert!(!dir.exists());
    }

    #[test]
    fn unconvertible_msg_reports_an_error_and_does_not_open_anything() {
        let spec = Spec {
            rows: 40,
            missing_every: 0,
            variant: Variant::Financial,
            ..Spec::default()
        };
        let (mut app, ctx, calls, _fx) = loaded_app(&spec);
        let msg_row = (0..40)
            .find(|&r| app.archive.as_ref().unwrap().row_kind(r) == FileKind::Msg)
            .unwrap();
        let path = app
            .archive
            .as_ref()
            .unwrap()
            .row_file(msg_row)
            .unwrap()
            .path
            .clone();
        std::fs::write(&path, b"garbage").unwrap();
        frame(&mut app, &ctx);
        app.open_row(msg_row, false);
        assert!(calls.borrow().is_empty());
        let e = app.error.clone().unwrap();
        assert!(e.contains("Could not convert") && e.contains("Open original"));
    }

    #[test]
    fn ctrl_wheel_zooms_the_interface() {
        let ctx = egui::Context::default();
        let mut app = App::new(None, recorder().0);
        app.remember_path = false;
        let wheel = |dy: f32, ctrl: bool| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1280.0, 760.0),
                )),
                ..Default::default()
            };
            input.events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, dy),
                modifiers: egui::Modifiers {
                    ctrl,
                    command: ctrl,
                    ..Default::default()
                },
            });
            input
        };
        // egui smooths wheel input over several frames, so run a few after the event.
        let mut clock = 0.0f64;
        let mut run = |ctx: &egui::Context, app: &mut App, mut input: egui::RawInput| {
            for n in 0..12 {
                clock += 0.016;
                input.time = Some(clock);
                let _ = ctx.run(input, |c| app.ui(c));
                input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1280.0, 760.0),
                    )),
                    ..Default::default()
                };
                let _ = n;
            }
        };
        let before = ctx.zoom_factor();
        run(&ctx, &mut app, wheel(1.0, true));
        let bigger = ctx.zoom_factor();
        assert!(bigger > before, "wheel up with Ctrl zooms in");
        run(&ctx, &mut app, wheel(-1.0, true));
        run(&ctx, &mut app, wheel(-1.0, true));
        assert!(ctx.zoom_factor() < bigger, "wheel down zooms out");
        for _ in 0..5 {
            run(&ctx, &mut app, egui::RawInput::default()); // let smoothing settle
        }
        let now = ctx.zoom_factor();
        run(&ctx, &mut app, wheel(1.0, false));
        assert_eq!(ctx.zoom_factor(), now, "plain wheel only scrolls");
        for _ in 0..60 {
            run(&ctx, &mut app, wheel(1.0, true));
        }
        assert!(ctx.zoom_factor() <= 4.0);
    }

    #[test]
    fn starts_empty_without_a_path_and_never_loads_by_itself() {
        let (op, _calls) = recorder();
        let mut app = App::new(None, op);
        app.remember_path = false;
        let ctx = egui::Context::default();
        for _ in 0..20 {
            frame(&mut app, &ctx);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(app.archive.is_none() && app.loading.is_none());
        assert!(app.rows.is_empty() && app.error.is_none());
    }

    #[test]
    fn folder_browser_renders_and_can_be_cancelled() {
        let fx = synth_archive(&Spec {
            rows: 5,
            pass_parent: true,
            ..Spec::default()
        })
        .unwrap();
        let (op, _calls) = recorder();
        let mut app = App::new(None, op);
        app.remember_path = false;
        let ctx = egui::Context::default();
        app.picker = Some(FolderPicker::new(fx.dir.path(), None));
        frame(&mut app, &ctx);
        assert!(app.picker.is_some());
        let mut esc = egui::RawInput::default();
        esc.events.push(egui::Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        });
        let _ = ctx.run(esc, |c| app.ui(c));
        assert!(app.picker.is_none(), "Escape closes the browser");
        assert!(app.archive.is_none() && app.loading.is_none());
    }
}
