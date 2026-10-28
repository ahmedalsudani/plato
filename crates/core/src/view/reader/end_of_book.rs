use std::path::{Path, PathBuf};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Framebuffer;
use crate::geom::{Rectangle, BorderSpec, CycleDir, small_half, big_half};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use crate::metadata::Info;
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, ViewId, Align};
use crate::view::{THICKNESS_MEDIUM, THICKNESS_LARGE};
use crate::view::label::Label;
use crate::view::button::Button;
use crate::view::icon::Icon;
use crate::view::filler::Filler;
use crate::gesture::GestureEvent;
use crate::color::{BLACK, WHITE, SEPARATOR_NORMAL, SEPARATOR_STRONG};
use crate::unit::scale_by_dpi;
use crate::context::Context;

const LABEL_TITLE: &str = "End of book";
const LABEL_LIBRARY: &str = "Library";

// The directions of the books of the rows, from top to bottom.
const DIRS: [CycleDir; 2] = [CycleDir::Previous, CycleDir::Next];

// The book that precedes or follows the one at `path` in `books`, the paths
// of the books of a home view, in the order they were shown. There's none at
// either end of the list, or if the book isn't in it.
pub fn neighbor_book<'a>(books: &'a [PathBuf], path: &Path, dir: CycleDir) -> Option<&'a Path> {
    let index = books.iter().position(|p| p == path)?;
    let neighbor = match dir {
        CycleDir::Next => index + 1,
        CycleDir::Previous => index.checked_sub(1)?,
    };
    books.get(neighbor).map(PathBuf::as_path)
}

// The text of the row that opens the book preceding or following the
// finished one: the book's title, which is the file name of an untitled
// book. The row of a missing book says so.
fn book_label(dir: CycleDir, info: Option<&Info>) -> String {
    match (dir, info) {
        (_, Some(info)) => info.title(),
        (CycleDir::Previous, None) => "No previous book".to_string(),
        (CycleDir::Next, None) => "No next book".to_string(),
    }
}

// The icon of the chevron in front of a book's title, pointing towards the
// book. It's as small as the marks of a menu's entries.
fn chevron_name(dir: CycleDir) -> &'static str {
    match dir {
        CycleDir::Previous => "angle-left-small",
        CycleDir::Next => "angle-right-small",
    }
}

// The sizes, in pixels, the window's layout depends on: the ones of a menu.
#[derive(Debug, Copy, Clone)]
struct Metrics {
    entry_height: i32,  // Height of a row, the one of a menu's entries.
    padding: i32,       // Padding of a menu's entries, twice the width of a gutter.
    em: i32,            // Padding of the labels and buttons.
    thickness: i32,     // Thickness of the separators.
    border: i32,        // Thickness of the window's border.
    label_width: i32,   // Width of the widest of the window's title and the library's label.
    text_width: i32,    // Width of the widest text of the rows.
}

impl Metrics {
    fn new(fonts: &mut Fonts, labels: &[String]) -> Metrics {
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
        let entry_height = font.x_heights.0 as i32 * 5;
        let em = font.em() as i32;
        let mut width = |text: &str| font.plan(text, None, None).width;
        let label_width = width(LABEL_TITLE).max(width(LABEL_LIBRARY));
        let text_width = labels.iter().map(|label| width(label)).fold(label_width, i32::max);
        Metrics {
            entry_height,
            padding: 4 * em,
            em,
            thickness: scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32,
            border: scale_by_dpi(THICKNESS_LARGE, dpi) as i32,
            label_width,
            text_width,
        }
    }
}

// The row of the previous or next book: the chevron in a narrow gutter on the
// left, a vertical line, then the book's title. The whole row opens the book.
#[derive(Debug, Copy, Clone)]
struct BookRow {
    rect: Rectangle,
    chevron: Rectangle,
    separator: Rectangle,
    title: Rectangle,
}

impl BookRow {
    // A button keeps its text an em from its edges, half of it on the left:
    // the title reaches into the right gutter by the other half, so that its
    // text stops at the gutter, like the text of a menu's entry.
    fn new(rect: Rectangle, gutter: i32, thickness: i32, em: i32) -> BookRow {
        let chevron = rect![rect.min.x, rect.min.y, rect.min.x + gutter, rect.max.y];
        let separator = rect![chevron.max.x, rect.min.y, chevron.max.x + thickness, rect.max.y];
        let title = rect![separator.max.x, rect.min.y, rect.max.x - gutter + em - em / 2, rect.max.y];
        BookRow { rect, chevron, separator, title }
    }
}

#[derive(Debug)]
struct Layout {
    rect: Rectangle,
    header: Rectangle,      // The window's title, left of the close icon.
    close: Rectangle,       // The right gutter of the top row.
    lines: [Rectangle; 2],  // The lines below the top row and above the library's row.
    books: [BookRow; 2],    // The rows of the previous and next books.
    library: Rectangle,
    text_x: i32,            // Where the texts of all the rows start.
}

impl Layout {
    // The rectangles of the window's children, in their order.
    fn children(&self) -> Vec<Rectangle> {
        let mut rects = vec![self.header, self.close, self.lines[0]];
        for book in &self.books {
            rects.extend([book.chevron, book.separator, book.title]);
        }
        rects.extend([self.lines[1], self.library]);
        rects
    }
}

// Centers the window on a display of the given dimensions. Inside its border,
// four rows spaced like the entries of a menu, the lines between them taking
// their thickness from the rows they separate: the window's title, with the
// close icon in the right gutter, a line, the rows of the previous and next
// books, a line, and the library's row. The texts of the rows start on the
// same column, past the gutter of the chevrons and its line, and stop at the
// right gutter. Like a menu, the window fits its widest text without growing
// past two thirds of the display's width, unless its title and the library's
// label need more room.
fn layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
    let Metrics { entry_height, padding, em, thickness, border, label_width, text_width } = *metrics;
    let (width, height) = (dims.0 as i32, dims.1 as i32);
    let gutter = padding / 2;
    // From the inside of the border to the texts: the gutter of the chevrons,
    // their line and the padding of a button.
    let indent = gutter + thickness + em / 2;

    let fit = |text_width: i32| 2 * border + indent + text_width + gutter;
    let window_width = fit(text_width).min(2 * width / 3)
                                      .max(fit(label_width))
                                      .min(width);
    let window_height = 4 * entry_height + 2 * border;
    let dx = (width - window_width) / 2;
    let dy = (height - window_height) / 2;
    let rect = rect![dx, dy, dx + window_width, dy + window_height];
    let inner = rect![rect.min.x + border, rect.min.y + border,
                      rect.max.x - border, rect.max.y - border];

    // The top of the row of the given index.
    let row_y = |index: i32| inner.min.y + index * entry_height;
    let lines = [1, 3].map(|index| {
        let y = row_y(index);
        rect![inner.min.x, y - small_half(thickness), inner.max.x, y + big_half(thickness)]
    });

    let header = rect![inner.min.x, inner.min.y, inner.max.x - gutter, lines[0].min.y];
    let close = rect![header.max.x, inner.min.y, inner.max.x, lines[0].min.y];
    let books = [rect![inner.min.x, lines[0].max.y, inner.max.x, row_y(2)],
                 rect![inner.min.x, row_y(2), inner.max.x, lines[1].min.y]]
                .map(|rect| BookRow::new(rect, gutter, thickness, em));
    let library = rect![inner.min.x, lines[1].max.y, inner.max.x, inner.max.y];

    Layout { rect, header, close, lines, books, library, text_x: inner.min.x + indent }
}

// Shown when a book is finished: opens the previous or next book of the home
// view the book was opened from, or goes back to the library. It's drawn like
// a menu, in a window with square corners.
pub struct EndOfBook {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    metrics: Metrics,
    // The rows of the previous and next books, when there's a book to open.
    books: [Option<Rectangle>; 2],
}

impl EndOfBook {
    pub fn new(previous: Option<&Info>, next: Option<&Info>, context: &mut Context) -> EndOfBook {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let infos = [previous, next];
        let labels = [0, 1].map(|index| book_label(DIRS[index], infos[index]));
        let metrics = Metrics::new(&mut context.fonts, &labels);
        let Layout { rect, header, close, lines, books, library, text_x } = layout(context.display.dims, &metrics);
        let sep_color = if context.fb.monochrome() { SEPARATOR_STRONG } else { SEPARATOR_NORMAL };
        let mut rows = [None; 2];

        // The children come in the order of Layout::children. The texts are
        // offset from the left of their cells to start on the same column.
        let label = Label::new(header, LABEL_TITLE.to_string(), Align::Left(text_x - header.min.x));
        children.push(Box::new(label) as Box<dyn View>);

        let close_icon = Icon::new("close-small", close, Event::Close(ViewId::EndOfBook));
        children.push(Box::new(close_icon) as Box<dyn View>);

        children.push(Box::new(Filler::new(lines[0], sep_color)) as Box<dyn View>);

        for (index, ((dir, info), label)) in DIRS.into_iter().zip(infos).zip(labels).enumerate() {
            let row = books[index];

            // A missing book leaves the gutter of its chevron empty.
            if info.is_some() {
                let icon = Icon::new(chevron_name(dir), row.chevron, Event::Book(dir));
                children.push(Box::new(icon) as Box<dyn View>);
                rows[index] = Some(row.rect);
            } else {
                let filler = Filler::new(row.chevron, WHITE);
                children.push(Box::new(filler) as Box<dyn View>);
            }

            let separator = Filler::new(row.separator, sep_color);
            children.push(Box::new(separator) as Box<dyn View>);

            // The button draws as much of the title as fits, followed by an ellipsis.
            let button = Button::new(row.title, Event::Book(dir), label)
                                .border(false)
                                .align(Align::Left(text_x - row.title.min.x))
                                .disabled(info.is_none());
            children.push(Box::new(button) as Box<dyn View>);
        }

        children.push(Box::new(Filler::new(lines[1], sep_color)) as Box<dyn View>);

        let button_library = Button::new(library, Event::Back, LABEL_LIBRARY.to_string())
                                    .border(false)
                                    .align(Align::Left(text_x - library.min.x));
        children.push(Box::new(button_library) as Box<dyn View>);

        EndOfBook {
            id,
            rect,
            children,
            metrics,
            books: rows,
        }
    }
}

impl View for EndOfBook {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) if !self.rect.includes(center) => {
                hub.send(Event::Close(ViewId::EndOfBook)).ok();
                true
            },
            // The chevrons and the titles take the taps on them: the line
            // between them and the right gutter are what's left of a book's
            // row.
            Event::Gesture(GestureEvent::Tap(center)) => {
                if let Some(index) = self.books.iter().position(|book| book.map_or(false, |rect| rect.includes(center))) {
                    bus.push_back(Event::Book(DIRS[index]));
                }
                true
            },
            Event::Gesture(..) => true,
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
        fb.draw_rectangle(&self.rect, WHITE);
        fb.draw_rectangle_outline(&self.rect, &BorderSpec { thickness: self.metrics.border as u16,
                                                            color: BLACK });
    }

    fn resize(&mut self, _rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let layout = layout(context.display.dims, &self.metrics);
        // The children keep their state: the row of a missing book stays
        // disabled. The texts keep their column, which is as far from the left
        // of their cells on any display.
        for (child, rect) in self.children.iter_mut().zip(layout.children()) {
            child.resize(rect, hub, rq, context);
        }
        for (book, row) in self.books.iter_mut().zip(layout.books.iter()) {
            if let Some(rect) = book {
                *rect = row.rect;
            }
        }
        self.rect = layout.rect;
    }

    fn is_background(&self) -> bool {
        true
    }

    fn rect(&self) -> &Rectangle {
        &self.rect
    }

    fn rect_mut(&mut self) -> &mut Rectangle {
        &mut self.rect
    }

    fn children(&self) -> &Vec<Box<dyn View>> {
        &self.children
    }

    fn children_mut(&mut self) -> &mut Vec<Box<dyn View>> {
        &mut self.children
    }

    fn id(&self) -> Id {
        self.id
    }

    fn view_id(&self) -> Option<ViewId> {
        Some(ViewId::EndOfBook)
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::collections::VecDeque;
    use std::sync::mpsc::{self, Receiver};
    use crate::battery::{Battery, FakeBattery};
    use crate::color::{BLACK, WHITE, SEPARATOR_NORMAL, TEXT_NORMAL};
    use crate::context::Context;
    use crate::device::CURRENT_DEVICE;
    use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
    use crate::framebuffer::{Framebuffer, Pixmap};
    use crate::frontlight::{Frontlight, LightLevels};
    use crate::gesture::GestureEvent;
    use crate::geom::{Point, Rectangle, CycleDir, Dir, small_half, big_half};
    use crate::library::Library;
    use crate::lightsensor::LightSensor;
    use crate::metadata::{Info, FileInfo};
    use crate::settings::{Settings, LibraryMode};
    use crate::view::{View, Event, Hub, ViewId, RenderQueue, EntryKind, EntryId, handle_event};
    use crate::view::button::Button;
    use crate::view::filler::Filler;
    use crate::view::icon::Icon;
    use crate::view::label::Label;
    use crate::view::menu::{Menu, MenuKind};
    use super::{neighbor_book, book_label, layout, Layout, BookRow, Metrics, EndOfBook};
    use super::{DIRS, LABEL_TITLE, LABEL_LIBRARY};

    fn books(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    // The previous and next books of `path`.
    fn neighbors<'a>(books: &'a [PathBuf], path: &str) -> (Option<&'a str>, Option<&'a str>) {
        let path = Path::new(path);
        let neighbor = |dir| neighbor_book(books, path, dir).and_then(Path::to_str);
        (neighbor(CycleDir::Previous), neighbor(CycleDir::Next))
    }

    #[test]
    fn test_neighbor_book_middle() {
        let books = books(&["a.epub", "b.epub", "c.epub"]);
        assert_eq!(neighbors(&books, "b.epub"), (Some("a.epub"), Some("c.epub")));
    }

    #[test]
    fn test_neighbor_book_first() {
        let books = books(&["a.epub", "b.epub", "c.epub"]);
        assert_eq!(neighbors(&books, "a.epub"), (None, Some("b.epub")));
    }

    #[test]
    fn test_neighbor_book_last() {
        let books = books(&["a.epub", "b.epub", "c.epub"]);
        assert_eq!(neighbors(&books, "c.epub"), (Some("b.epub"), None));
    }

    #[test]
    fn test_neighbor_book_missing() {
        let books = books(&["a.epub", "b.epub", "c.epub"]);
        assert_eq!(neighbors(&books, "d.epub"), (None, None));
    }

    #[test]
    fn test_neighbor_book_single() {
        let books = books(&["a.epub"]);
        assert_eq!(neighbors(&books, "a.epub"), (None, None));
    }

    #[test]
    fn test_neighbor_book_empty() {
        let books = books(&[]);
        assert_eq!(neighbors(&books, "a.epub"), (None, None));
    }

    #[test]
    fn test_neighbor_book_follows_the_list_order() {
        // The list keeps the order of the home view (here, by date opened):
        // the neighbors aren't the alphabetical ones.
        let books = books(&["c.epub", "a.epub", "b.epub"]);
        assert_eq!(neighbors(&books, "a.epub"), (Some("c.epub"), Some("b.epub")));
        let reversed = books.iter().rev().cloned().collect::<Vec<PathBuf>>();
        assert_eq!(neighbors(&reversed, "a.epub"), (Some("b.epub"), Some("c.epub")));
    }

    #[test]
    fn test_neighbor_book_compares_whole_paths() {
        // Search results span directories: the books are told apart by
        // their path relative to the library, not by their file name.
        let books = books(&["Fiction/book.epub", "Essays/book.epub", "Essays/other.epub"]);
        assert_eq!(neighbors(&books, "Essays/book.epub"),
                   (Some("Fiction/book.epub"), Some("Essays/other.epub")));
        assert_eq!(neighbors(&books, "book.epub"), (None, None));
    }

    fn info(title: &str, path: &str) -> Info {
        Info {
            title: title.to_string(),
            file: FileInfo {
                path: PathBuf::from(path),
                kind: "epub".to_string(),
                size: 0,
            },
            .. Default::default()
        }
    }

    #[test]
    fn test_book_label_titles() {
        // The chevron, an icon, isn't part of the text.
        let dune = info("Dune", "Fiction/dune.epub");
        assert_eq!(book_label(CycleDir::Previous, Some(&dune)), "Dune");
        assert_eq!(book_label(CycleDir::Next, Some(&dune)), "Dune");
    }

    #[test]
    fn test_book_label_falls_back_to_file_name() {
        let untitled = info("", "Fiction/dune-messiah.epub");
        assert_eq!(book_label(CycleDir::Previous, Some(&untitled)), "dune-messiah");
        assert_eq!(book_label(CycleDir::Next, Some(&untitled)), "dune-messiah");
    }

    #[test]
    fn test_book_label_missing_book() {
        assert_eq!(book_label(CycleDir::Previous, None), "No previous book");
        assert_eq!(book_label(CycleDir::Next, None), "No next book");
    }

    // Sizes at 300 DPI.
    const METRICS: Metrics = Metrics {
        entry_height: 90,
        padding: 136,
        em: 34,
        thickness: 2,
        border: 3,
        label_width: 180,
        text_width: 400,
    };

    // The inside of the window, within its border.
    fn inside(rect: &Rectangle, border: i32) -> Rectangle {
        rect![rect.min.x + border, rect.min.y + border,
              rect.max.x - border, rect.max.y - border]
    }

    // The width of a window fitted to a text of the given width: a menu's
    // padding and border around the text, the line of the chevrons' gutter
    // and the padding of a button after it.
    fn fitted_width(metrics: &Metrics, text_width: i32) -> i32 {
        let Metrics { padding, em, thickness, border, .. } = *metrics;
        2 * border + padding + text_width + thickness + em / 2
    }

    // Checks the invariants of a layout and returns it.
    fn check_layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
        let layout = layout(dims, metrics);
        let Layout { rect, header, close, lines, books, library, text_x } = &layout;
        let Metrics { entry_height, padding, em, thickness, border, label_width, .. } = *metrics;
        let gutter = padding / 2;
        let screen = rect![0, 0, dims.0 as i32, dims.1 as i32];

        // The window is centered on the display.
        assert!(screen.contains(rect), "{} isn't inside {}", rect, screen);
        assert!((rect.min.x - (screen.max.x - rect.max.x)).abs() <= 1);
        assert!((rect.min.y - (screen.max.y - rect.max.y)).abs() <= 1);

        // Four rows as tall as a menu's entries fill the inside of the border.
        let inner = inside(rect, border);
        assert_eq!(inner.height() as i32, 4 * entry_height);
        let row_y = |index: i32| inner.min.y + index * entry_height;

        // The lines below the top row and above the library's row span the
        // inside of the window, centered on the bounds of the rows they
        // separate, like a menu's.
        for (line, index) in lines.iter().zip([1, 3]) {
            let y = row_y(index);
            assert_eq!(*line, rect![inner.min.x, y - small_half(thickness), inner.max.x, y + big_half(thickness)]);
        }

        // The top row: the title, then the close icon in the right gutter.
        assert_eq!(*header, rect![inner.min.x, inner.min.y, inner.max.x - gutter, lines[0].min.y]);
        assert_eq!(*close, rect![header.max.x, inner.min.y, inner.max.x, lines[0].min.y]);

        // The rows of the books, with no line between them: the gutter of
        // the chevron, a vertical line as tall as the row, then the title,
        // whose text stops at the right gutter.
        assert_eq!(books[0].rect, rect![inner.min.x, lines[0].max.y, inner.max.x, row_y(2)]);
        assert_eq!(books[1].rect, rect![inner.min.x, row_y(2), inner.max.x, lines[1].min.y]);
        for BookRow { rect: row, chevron, separator, title } in books {
            assert_eq!(*chevron, rect![row.min.x, row.min.y, row.min.x + gutter, row.max.y]);
            assert_eq!(*separator, rect![chevron.max.x, row.min.y, chevron.max.x + thickness, row.max.y]);
            assert_eq!((title.min.x, title.min.y, title.max.y), (separator.max.x, row.min.y, row.max.y));
            assert_eq!(*text_x + title.width() as i32 - em, close.min.x);
        }

        // The library's row, at the bottom.
        assert_eq!(*library, rect![inner.min.x, lines[1].max.y, inner.max.x, inner.max.y]);

        // The texts start on the column of the titles, a button's padding
        // past the vertical lines. The window's title and the library's
        // label fit before the right gutter.
        assert_eq!(*text_x, books[0].title.min.x + em / 2);
        assert!(text_x + label_width <= close.min.x);

        // The children stay off the border and off each other.
        let rects = layout.children();
        for (index, a) in rects.iter().enumerate() {
            assert!(inner.contains(a), "{} isn't inside {}", a, inner);
            for b in &rects[index + 1..] {
                assert!(!a.overlaps(b), "{} overlaps {}", a, b);
            }
        }

        layout
    }

    #[test]
    fn test_layout_portrait_and_landscape() {
        // Short titles: the window fits them, and is as wide in both
        // orientations.
        for dims in [(1072, 1448), (1448, 1072)] {
            let Layout { rect, .. } = check_layout(dims, &METRICS);
            assert_eq!(rect.width() as i32, fitted_width(&METRICS, METRICS.text_width));
            assert_eq!(rect.height() as i32, 4 * METRICS.entry_height + 2 * METRICS.border);
        }
    }

    #[test]
    fn test_layout_long_titles() {
        // The window grows to two thirds of the display's width, like a
        // menu, and the titles are too long for their buttons.
        let metrics = Metrics { text_width: 2000, .. METRICS };
        for dims in [(1072, 1448), (1448, 1072)] {
            let Layout { rect, books, .. } = check_layout(dims, &metrics);
            assert_eq!(rect.width() as i32, 2 * dims.0 as i32 / 3);
            for book in books {
                assert!((book.title.width() as i32) - metrics.em < metrics.text_width);
            }
        }
    }

    #[test]
    fn test_layout_narrow_display() {
        // Two thirds of the display's width are too narrow for the window's
        // title: the window grows to fit it, but not the books' titles.
        let metrics = Metrics { label_width: 600, text_width: 2000, .. METRICS };
        let Layout { rect, .. } = check_layout((800, 600), &metrics);
        assert_eq!(rect.width() as i32, fitted_width(&metrics, metrics.label_width));
        assert!(rect.width() as i32 > 2 * 800 / 3);
    }

    // A context on a display of the given dimensions, with the fonts of the
    // repository and an empty library.
    struct Fixture {
        dir: PathBuf,
        context: Context,
        hub: Hub,
        receiver: Receiver<Event>,
    }

    impl Fixture {
        fn new(name: &str, dims: (u32, u32)) -> Fixture {
            let dir = env::temp_dir().join(format!("plato-end-of-book-{}-{}", name, std::process::id()));
            fs::remove_dir_all(&dir).ok();
            let library = Library::new(&dir, LibraryMode::Filesystem).unwrap();
            let fonts = Fonts::load_from(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fonts")).unwrap();
            let fb = Box::new(Pixmap::new(dims.0, dims.1, 1)) as Box<dyn Framebuffer>;
            let context = Context::new(fb, None, library, Settings::default(), fonts,
                                       Box::new(FakeBattery::new()) as Box<dyn Battery>,
                                       Box::new(LightLevels::default()) as Box<dyn Frontlight>,
                                       Box::new(0u16) as Box<dyn LightSensor>);
            let (hub, receiver) = mpsc::channel();
            Fixture { dir, context, hub, receiver }
        }

        // Whether the window captures `evt`, and the events it sends to its
        // parent then.
        fn handle(&mut self, view: &mut EndOfBook, evt: &Event) -> (bool, Vec<Event>) {
            let mut bus = VecDeque::new();
            let captured = handle_event(view, evt, &self.hub, &mut bus, &mut RenderQueue::new(), &mut self.context);
            (captured, bus.into_iter().collect())
        }

        // The events a tap at `pt` sends to the window's parent. The window
        // captures every tap.
        fn tap(&mut self, view: &mut EndOfBook, pt: Point) -> Vec<Event> {
            let (captured, events) = self.handle(view, &Event::Gesture(GestureEvent::Tap(pt)));
            assert!(captured, "the tap at {:?} went through", pt);
            events
        }

        // The width of `text` in the window's font.
        fn text_width(&mut self, text: &str) -> i32 {
            let font = font_from_style(&mut self.context.fonts, &NORMAL_STYLE, CURRENT_DEVICE.dpi);
            font.plan(text, None, None).width
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.dir).ok();
        }
    }

    fn button(view: &EndOfBook, index: usize) -> &Button {
        view.children()[index].downcast_ref::<Button>().unwrap()
    }

    fn icon(view: &EndOfBook, index: usize) -> &Icon {
        view.children()[index].downcast_ref::<Icon>().unwrap()
    }

    // The kinds of the window's children, in their order.
    fn kinds(view: &EndOfBook) -> Vec<&'static str> {
        view.children().iter().map(|child| {
            if child.is::<Button>() {
                "button"
            } else if child.is::<Icon>() {
                "icon"
            } else if child.is::<Filler>() {
                "filler"
            } else if child.is::<Label>() {
                "label"
            } else {
                "other"
            }
        }).collect()
    }

    fn rects(view: &EndOfBook) -> Vec<Rectangle> {
        view.children().iter().map(|child| *child.rect()).collect()
    }

    // Whether the taps at `pts` all open the book of `dir`.
    fn all_open(f: &mut Fixture, view: &mut EndOfBook, pts: &[Point], dir: CycleDir) -> bool {
        pts.iter().all(|pt| matches!(f.tap(view, *pt).as_slice(), [Event::Book(d)] if *d == dir))
    }

    // Points across a row, from its left edge to its right one.
    fn across(row: &Rectangle) -> Vec<Point> {
        let y = row.center().y;
        (row.min.x..row.max.x).map(|x| pt!(x, y)).collect()
    }

    #[test]
    fn test_rows_match_the_menu() {
        let mut f = Fixture::new("menu", (1072, 1448));
        let previous = info("Dune", "dune.epub");
        let next = info("Children of Dune", "children-of-dune.epub");
        let view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);

        // A menu of the window's texts, with a line below the first one.
        let texts = [LABEL_TITLE, "Dune", "Children of Dune", LABEL_LIBRARY];
        let mut entries = texts.iter().map(|text| EntryKind::Command(text.to_string(), EntryId::About))
                               .collect::<Vec<EntryKind>>();
        entries.insert(1, EntryKind::Separator);
        let menu = Menu::new(rect![500, 100, 572, 160], ViewId::MainMenu, MenuKind::Contextual, entries, &mut f.context);
        let (line, entry) = (*menu.children()[1].rect(), *menu.children()[3].rect());

        // The rows are as tall as its entries, the lines and the border as
        // thick as its own.
        let Metrics { entry_height, thickness, border, em, .. } = view.metrics;
        assert_eq!(entry.height() as i32, entry_height);
        assert_eq!(line.height() as i32, thickness);
        assert_eq!(entry.min.x - menu.rect().min.x, border);

        // The window is as wide as the menu, but for the line of the
        // chevrons' gutter and the padding of the titles after it.
        assert_eq!(view.rect.width() as i32, menu.rect().width() as i32 + thickness + em / 2);
    }

    #[test]
    fn test_window_children() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("children", dims);
        let previous = info("Dune", "dune.epub");
        let next = info("Children of Dune", "children-of-dune.epub");
        let mut view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);
        let expected = layout(dims, &view.metrics);
        assert_eq!(rects(&view), expected.children());
        assert_eq!(kinds(&view), ["label", "icon", "filler",
                                  "icon", "filler", "button",
                                  "icon", "filler", "button",
                                  "filler", "button"]);
        // The icons are the small ones of the menus.
        assert_eq!(icon(&view, 1).name, "close-small");
        assert_eq!(icon(&view, 3).name, "angle-left-small");
        assert_eq!(icon(&view, 6).name, "angle-right-small");

        assert!(matches!(f.tap(&mut view, expected.close.center()).as_slice(), [Event::Close(ViewId::EndOfBook)]));

        // The whole library's row goes back to the library.
        for pt in across(&expected.library) {
            assert!(matches!(f.tap(&mut view, pt).as_slice(), [Event::Back]), "{:?}", pt);
        }

        // The title and the lines don't respond.
        for rect in [expected.header, expected.lines[0], expected.lines[1]] {
            for pt in across(&rect) {
                assert!(f.tap(&mut view, pt).is_empty(), "the tap at {:?} responds", pt);
            }
        }
    }

    #[test]
    fn test_book_rows() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("rows", dims);
        let previous = info("Dune", "dune.epub");
        let next = info("", "children-of-dune.epub");
        let mut view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);
        let Layout { books, .. } = layout(dims, &view.metrics);

        // Each row starts with a chevron pointing towards its book, then a
        // line, then the title.
        assert_eq!(kinds(&view)[3..9], ["icon", "filler", "button",
                                        "icon", "filler", "button"]);
        assert!(!button(&view, 5).disabled && !button(&view, 8).disabled);

        // The chevron and the title send the same event, and so does the line
        // between them: the whole row opens the book.
        for (dir, row) in DIRS.into_iter().zip(books) {
            let pts = [row.chevron.center(), row.title.center(), row.separator.center()];
            assert!(all_open(&mut f, &mut view, &pts, dir), "{:?}", dir);
            assert!(all_open(&mut f, &mut view, &across(&row.rect), dir), "{:?}", dir);
        }
    }

    #[test]
    fn test_missing_book_row() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("missing", dims);
        let next = info("Dune", "dune.epub");
        let mut view = EndOfBook::new(None, Some(&next), &mut f.context);
        let Layout { books, .. } = layout(dims, &view.metrics);

        // There's no previous book: its row stays, with an empty chevron
        // gutter and a disabled title, and nothing on it responds.
        assert_eq!(kinds(&view)[3..6], ["filler", "filler", "button"]);
        assert!(button(&view, 5).disabled);
        for pt in across(&books[0].rect) {
            assert!(f.tap(&mut view, pt).is_empty(), "the tap at {:?} responds", pt);
        }

        // The row of the next book works.
        assert_eq!(icon(&view, 6).name, "angle-right-small");
        assert!(!button(&view, 8).disabled);
        assert!(all_open(&mut f, &mut view, &across(&books[1].rect), CycleDir::Next));
    }

    #[test]
    fn test_taps_outside_close_the_window() {
        let mut f = Fixture::new("outside", (1072, 1448));
        let mut view = EndOfBook::new(None, None, &mut f.context);
        let rect = view.rect;
        for pt in [pt!(0, 0), pt!(rect.min.x - 1, rect.center().y), pt!(rect.center().x, rect.max.y)] {
            assert!(f.tap(&mut view, pt).is_empty());
            assert!(matches!(f.receiver.try_recv(), Ok(Event::Close(ViewId::EndOfBook))), "{:?}", pt);
        }

        // The gestures inside, the border included, are swallowed.
        for pt in [rect.min, rect.center(), pt!(rect.max.x - 1, rect.center().y)] {
            assert!(f.tap(&mut view, pt).is_empty());
            let swipe = Event::Gesture(GestureEvent::Swipe { dir: Dir::West, start: pt, end: pt!(0, pt.y) });
            let (captured, events) = f.handle(&mut view, &swipe);
            assert!(captured && events.is_empty(), "{:?}", pt);
        }
        assert!(f.receiver.try_recv().is_err());
    }

    #[test]
    fn test_width_fits_the_texts() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("width", dims);
        let dune = info("Dune", "dune.epub");
        let alpha = info("Alpha: A Short First Book", "alpha.epub");

        // The window fits the widest text of its rows, which can be the
        // text of a missing book, and stays narrower than a long menu.
        let narrow = EndOfBook::new(Some(&dune), None, &mut f.context);
        let widest = ["End of book", "Library", "Dune", "No next book"].map(|text| f.text_width(text))
                                                                        .into_iter().max().unwrap();
        assert_eq!(narrow.rect.width() as i32, fitted_width(&narrow.metrics, widest));

        let wide = EndOfBook::new(Some(&dune), Some(&alpha), &mut f.context);
        let widest = f.text_width("Alpha: A Short First Book");
        assert_eq!(wide.rect.width() as i32, fitted_width(&wide.metrics, widest));
        assert!(narrow.rect.width() < wide.rect.width());
        assert!((wide.rect.width() as i32) < 2 * dims.0 as i32 / 3);

        // The title it fits isn't cut short: the button has room for it.
        let title = button(&wide, 8);
        assert_eq!(title.rect().width() as i32 - wide.metrics.em, widest);
    }

    #[test]
    fn test_long_titles_are_truncated() {
        let mut f = Fixture::new("truncated", (1072, 1448));
        let title = "The Very Long Title of a Book That Goes On and On ".repeat(4);
        let long = info(title.trim_end(), "long.epub");
        let short = info("Dune", "dune.epub");
        let view = EndOfBook::new(Some(&long), Some(&short), &mut f.context);

        // The window is as wide as a menu can be.
        assert_eq!(view.rect.width() as i32, 2 * 1072 / 3);

        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(&mut f.context.fonts, &NORMAL_STYLE, dpi);
        let padding = font.em() as i32;
        for (index, dir, info, truncated) in [(5, CycleDir::Previous, &long, true),
                                              (8, CycleDir::Next, &short, false)] {
            let rect = *view.children()[index].rect();
            // The plan the button renders: as much of the title as fits the
            // cell, followed by an ellipsis.
            let label = book_label(dir, Some(info));
            let max_width = rect.width() as i32 - padding;
            let full = font.plan(&label, None, None);
            let plan = font.plan(&label, Some(max_width), None);
            assert!(plan.width <= max_width);
            assert_eq!(full.width > max_width, truncated);
            assert_eq!(plan.width < full.width, truncated);
        }

        // Nothing is drawn outside of the cell, and the text stops at the
        // right gutter, below the close icon's.
        let rect = *view.children()[5].rect();
        let close = *view.children()[1].rect();
        let mut pixmap = Pixmap::new(1072, 1448, 1);
        view.children()[5].render(&mut pixmap, rect, &mut f.context.fonts);
        for y in 0..pixmap.height {
            for x in 0..pixmap.width {
                let pt = pt!(x as i32, y as i32);
                if !rect.includes(pt) || pt.x >= close.min.x {
                    assert_eq!(pixmap.get_pixel(x, y), WHITE, "({}, {}) is outside of {}", x, y, rect);
                }
            }
        }
    }

    // Renders the window and its children into a pixmap of the display's
    // size. The icons are left out: their pixmaps are loaded from the working
    // directory, which isn't the repository's root in the tests.
    fn render(view: &EndOfBook, fonts: &mut Fonts, dims: (u32, u32)) -> Pixmap {
        let mut pixmap = Pixmap::new(dims.0, dims.1, 1);
        view.render(&mut pixmap, view.rect, fonts);
        for child in view.children().iter().filter(|child| !child.is::<Icon>()) {
            child.render(&mut pixmap, *child.rect(), fonts);
        }
        pixmap
    }

    // The pixels of `rect`, with their coordinates.
    fn pixels(pixmap: &Pixmap, rect: &Rectangle) -> Vec<(Point, u8)> {
        let mut pixels = Vec::new();
        for y in rect.min.y..rect.max.y {
            for x in rect.min.x..rect.max.x {
                pixels.push((pt!(x, y), pixmap.get_pixel(x as u32, y as u32).gray()));
            }
        }
        pixels
    }

    // Checks that the texts of the rows start on the column of the layout,
    // give or take the side bearings of their first letters.
    fn check_texts_aligned(pixmap: &Pixmap, layout: &Layout, em: i32) {
        let Layout { header, books, library, text_x, .. } = layout;
        for cell in [*header, books[0].title, books[1].title, *library] {
            let start = pixels(pixmap, &cell).into_iter()
                                              .filter(|(_, gray)| *gray < WHITE.gray())
                                              .map(|(pt, _)| pt.x).min().expect("no text");
            assert!((start - text_x).abs() <= em / 4, "the text of {} starts at {}, not {}", cell, start, text_x);
        }
    }

    #[test]
    fn test_render_plain_window() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("render", dims);
        let previous = info("Dune", "dune.epub");
        let view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let metrics = view.metrics;
        let expected = layout(dims, &metrics);
        let Layout { rect, books, library, lines, .. } = expected;
        let pixmap = render(&view, &mut f.context.fonts, dims);
        let inner = inside(&rect, metrics.border);

        // A black border as thick as a menu's, with square corners, and
        // nothing outside of it.
        for (pt, gray) in pixels(&pixmap, &rect![0, 0, dims.0 as i32, dims.1 as i32]) {
            if !rect.includes(pt) {
                assert_eq!(gray, WHITE.gray(), "{:?} is outside of {}", pt, rect);
            } else if !inner.includes(pt) {
                assert_eq!(gray, BLACK.gray(), "{:?} is on the border of {}", pt, rect);
            }
        }

        // The lines, the vertical ones included, are fillers of the
        // separators' color.
        for line in [lines[0], books[0].separator, books[1].separator, lines[1]] {
            assert!(view.children().iter().any(|child| child.is::<Filler>() && *child.rect() == line));
            for (pt, gray) in pixels(&pixmap, &line) {
                assert_eq!(gray, SEPARATOR_NORMAL.gray(), "{:?} isn't on a line", pt);
            }
        }

        // The buttons are flat: the edges of their cells are white.
        for cell in [books[0].title, books[1].title, library] {
            let edges = [rect![cell.min.x, cell.min.y, cell.max.x, cell.min.y + 1],
                         rect![cell.min.x, cell.max.y - 1, cell.max.x, cell.max.y],
                         rect![cell.min.x, cell.min.y, cell.min.x + 1, cell.max.y],
                         rect![cell.max.x - 1, cell.min.y, cell.max.x, cell.max.y]];
            for edge in edges {
                for (pt, gray) in pixels(&pixmap, &edge) {
                    assert_eq!(gray, WHITE.gray(), "{:?} is on an outline of {}", pt, cell);
                }
            }
        }

        // The title of the previous book is black, the text of the missing
        // next book is grayed out.
        let darkest = |cell: &Rectangle| pixels(&pixmap, cell).into_iter().map(|(_, gray)| gray).min().unwrap();
        assert!(darkest(&books[0].title) < TEXT_NORMAL[2].gray());
        let grayed = darkest(&books[1].title);
        assert!(grayed >= TEXT_NORMAL[2].gray() && grayed < WHITE.gray(), "{}", grayed);

        check_texts_aligned(&pixmap, &expected, metrics.em);
    }

    #[test]
    fn test_resize() {
        let mut f = Fixture::new("resize", (1072, 1448));
        let title = "The Very Long Title of a Book That Goes On and On ".repeat(4);
        let previous = info(title.trim_end(), "long.epub");
        let mut view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let metrics = view.metrics;

        for dims in [(1448, 1072), (600, 800), (1072, 1448)] {
            f.context.display.dims = dims;
            view.resize(rect![0, 0, dims.0 as i32, dims.1 as i32], &f.hub, &mut RenderQueue::new(), &mut f.context);
            let expected = layout(dims, &metrics);
            assert_eq!(view.rect, expected.rect);
            assert_eq!(rects(&view), expected.children());
            // The long title takes as much room as a menu can.
            assert_eq!(view.rect.width() as i32, 2 * dims.0 as i32 / 3);

            // The rows keep their state.
            assert_eq!(icon(&view, 3).name, "angle-left-small");
            assert!(view.children()[6].is::<Filler>());
            assert!(!button(&view, 5).disabled && button(&view, 8).disabled);
            let [previous_row, next_row] = expected.books;
            assert!(all_open(&mut f, &mut view, &across(&previous_row.rect), CycleDir::Previous), "{:?}", dims);
            for pt in across(&next_row.rect) {
                assert!(f.tap(&mut view, pt).is_empty(), "the tap at {:?} responds", pt);
            }

            // The texts keep their column.
            let pixmap = render(&view, &mut f.context.fonts, dims);
            check_texts_aligned(&pixmap, &expected, metrics.em);
        }
    }
}
