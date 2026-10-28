use std::path::{Path, PathBuf};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Framebuffer;
use crate::geom::{Rectangle, BorderSpec, CycleDir};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use crate::metadata::Info;
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, ViewId, Align};
use crate::view::{BIG_BAR_HEIGHT, THICKNESS_MEDIUM};
use crate::view::label::Label;
use crate::view::button::Button;
use crate::view::icon::Icon;
use crate::view::filler::Filler;
use crate::gesture::GestureEvent;
use crate::color::{BLACK, WHITE, SEPARATOR_NORMAL};
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

// The icon of the chevron in front of a book's title, pointing towards the book.
fn chevron_name(dir: CycleDir) -> &'static str {
    match dir {
        CycleDir::Previous => "angle-left",
        CycleDir::Next => "angle-right",
    }
}

// The sizes, in pixels, the window's layout depends on.
#[derive(Debug, Copy, Clone)]
struct Metrics {
    row_height: i32,    // Height of a row, the one of the tool bar's rows.
    thickness: i32,     // Thickness of the window's border and of the separators.
    padding: i32,
    label_width: i32,   // Width of the library button's label.
    title_width: i32,   // Width of the title.
}

impl Metrics {
    fn new(fonts: &mut Fonts) -> Metrics {
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        Metrics {
            row_height: scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32 - thickness,
            thickness,
            padding: font.em() as i32,
            label_width: font.plan(LABEL_LIBRARY, None, None).width,
            title_width: font.plan(LABEL_TITLE, None, None).width,
        }
    }
}

// The row of the previous or next book: the square cell of the chevron on the
// left, a vertical line, then the book's title. The whole row opens the book.
#[derive(Debug, Copy, Clone)]
struct BookRow {
    rect: Rectangle,
    chevron: Rectangle,
    separator: Rectangle,
    title: Rectangle,
}

impl BookRow {
    fn new(rect: Rectangle, thickness: i32) -> BookRow {
        let side = rect.height() as i32;
        let chevron = rect![rect.min.x, rect.min.y, rect.min.x + side, rect.max.y];
        let separator = rect![chevron.max.x, rect.min.y, chevron.max.x + thickness, rect.max.y];
        let title = rect![separator.max.x, rect.min.y, rect.max.x, rect.max.y];
        BookRow { rect, chevron, separator, title }
    }
}

#[derive(Debug)]
struct Layout {
    rect: Rectangle,
    library: Rectangle,
    close: Rectangle,
    title: Option<Rectangle>,   // None when the title doesn't fit.
    separators: [Rectangle; 2], // The vertical lines of the top row.
    lines: [Rectangle; 2],      // The horizontal lines above the rows of the books.
    books: [BookRow; 2],        // The rows of the previous and next books.
}

impl Layout {
    // The rectangles of the window's children, in their order. The title,
    // when there's one, comes after them.
    fn children(&self) -> Vec<Rectangle> {
        let mut rects = vec![self.library, self.separators[0], self.separators[1], self.close];
        for (line, book) in self.lines.iter().zip(self.books.iter()) {
            rects.extend([*line, book.chevron, book.separator, book.title]);
        }
        rects
    }
}

// Centers the window on a display of the given dimensions. Inside its border,
// three rows as tall as the tool bar's, separated by horizontal lines. The top
// row holds the library button on the left and the close icon, in a square
// cell, on the right, each set apart by a vertical line, and the title,
// centered, if it fits between them. Below it, the rows of the previous and
// next books.
fn layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
    let Metrics { row_height, thickness, padding, label_width, title_width } = *metrics;
    let (width, height) = (dims.0 as i32, dims.1 as i32);

    // The window is as wide in both orientations, unless the top row needs
    // more room. The title keeps the same distance to both sides of the
    // window, the one of the widest cell and its line.
    let library_width = label_width + 2 * padding;
    let title_inset = library_width.max(row_height) + thickness;
    let top_row_width = 2 * (thickness + title_inset) + title_width + padding;
    let window_width = (5 * width.min(height) / 6).max(top_row_width)
                                                  .min(width - 2 * padding);
    let window_height = 3 * row_height + 4 * thickness;
    let dx = (width - window_width) / 2;
    let dy = (height - window_height) / 2;
    let rect = rect![dx, dy, dx + window_width, dy + window_height];
    let inner = rect![rect.min.x + thickness, rect.min.y + thickness,
                      rect.max.x - thickness, rect.max.y - thickness];

    let (min_y, max_y) = (inner.min.y, inner.min.y + row_height);
    let close = rect![inner.max.x - row_height, min_y, inner.max.x, max_y];

    // The library button shrinks when its label doesn't fit the display,
    // keeping the vertical lines apart.
    let library_width = library_width.min(close.min.x - 2 * thickness - padding - inner.min.x);
    let library = rect![inner.min.x, min_y, inner.min.x + library_width, max_y];

    let separators = [rect![library.max.x, min_y, library.max.x + thickness, max_y],
                      rect![close.min.x - thickness, min_y, close.min.x, max_y]];

    let title = if inner.width() as i32 - 2 * title_inset >= title_width + padding {
        Some(rect![inner.min.x + title_inset, min_y, inner.max.x - title_inset, max_y])
    } else {
        None
    };

    let lines = [0, 1].map(|i| {
        let min_y = inner.min.y + (i + 1) * row_height + i * thickness;
        rect![inner.min.x, min_y, inner.max.x, min_y + thickness]
    });

    let books = lines.map(|line| {
        BookRow::new(rect![inner.min.x, line.max.y, inner.max.x, line.max.y + row_height], thickness)
    });

    Layout { rect, library, close, title, separators, lines, books }
}

// Shown when a book is finished: opens the previous or next book of the home
// view the book was opened from, or goes back to the library. It's drawn like
// the reader's tool bar, in a plain window.
pub struct EndOfBook {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
    // The rows of the previous and next books, when there's a book to open.
    books: [Option<Rectangle>; 2],
}

impl EndOfBook {
    pub fn new(previous: Option<&Info>, next: Option<&Info>, context: &mut Context) -> EndOfBook {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let metrics = Metrics::new(&mut context.fonts);
        let Layout { rect, library, close, title, separators, lines, books } = layout(context.display.dims, &metrics);
        let mut rows = [None; 2];

        // The children come in the order of Layout::children.
        let button_library = Button::new(library, Event::Back, LABEL_LIBRARY.to_string())
                                    .border(false);
        children.push(Box::new(button_library) as Box<dyn View>);

        for separator in separators {
            let filler = Filler::new(separator, SEPARATOR_NORMAL);
            children.push(Box::new(filler) as Box<dyn View>);
        }

        let close_icon = Icon::new("close", close, Event::Close(ViewId::EndOfBook));
        children.push(Box::new(close_icon) as Box<dyn View>);

        for (index, (dir, info)) in DIRS.into_iter().zip([previous, next]).enumerate() {
            let row = books[index];
            let line = Filler::new(lines[index], SEPARATOR_NORMAL);
            children.push(Box::new(line) as Box<dyn View>);

            // A missing book leaves the cell of its chevron empty, like the
            // page arrows of the bottom bar.
            if info.is_some() {
                let icon = Icon::new(chevron_name(dir), row.chevron, Event::Book(dir));
                children.push(Box::new(icon) as Box<dyn View>);
                rows[index] = Some(row.rect);
            } else {
                let filler = Filler::new(row.chevron, WHITE);
                children.push(Box::new(filler) as Box<dyn View>);
            }

            let separator = Filler::new(row.separator, SEPARATOR_NORMAL);
            children.push(Box::new(separator) as Box<dyn View>);

            // The button draws as much of the title as fits, followed by an ellipsis.
            let button = Button::new(row.title, Event::Book(dir), book_label(dir, info))
                                .border(false)
                                .align(Align::Left(metrics.padding / 2))
                                .disabled(info.is_none());
            children.push(Box::new(button) as Box<dyn View>);
        }

        if let Some(title) = title {
            let label = Label::new(title, LABEL_TITLE.to_string(), Align::Center);
            children.push(Box::new(label) as Box<dyn View>);
        }

        EndOfBook {
            id,
            rect,
            children,
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
            // between them is what's left of a book's row.
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
        let dpi = CURRENT_DEVICE.dpi;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as u16;

        fb.draw_rectangle(&self.rect, WHITE);
        fb.draw_rectangle_outline(&self.rect, &BorderSpec { thickness, color: BLACK });
    }

    fn resize(&mut self, _rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let metrics = Metrics::new(&mut context.fonts);
        let layout = layout(context.display.dims, &metrics);
        // The children keep their state: the row of a missing book stays disabled.
        let rects = layout.children();
        for (child, rect) in self.children.iter_mut().zip(rects.iter()) {
            child.resize(*rect, hub, rq, context);
        }
        for (book, row) in self.books.iter_mut().zip(layout.books.iter()) {
            if let Some(rect) = book {
                *rect = row.rect;
            }
        }
        // The title, last, comes and goes with the room it has.
        self.children.truncate(rects.len());
        if let Some(title) = layout.title {
            let label = Label::new(title, LABEL_TITLE.to_string(), Align::Center);
            self.children.push(Box::new(label) as Box<dyn View>);
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
    use crate::geom::{Point, Rectangle, CycleDir, Dir};
    use crate::library::Library;
    use crate::lightsensor::LightSensor;
    use crate::metadata::{Info, FileInfo};
    use crate::settings::{Settings, ReaderSettings, LibraryMode};
    use crate::unit::scale_by_dpi;
    use crate::view::{View, Event, Hub, ViewId, RenderQueue, handle_event};
    use crate::view::{BIG_BAR_HEIGHT, THICKNESS_MEDIUM};
    use crate::view::button::Button;
    use crate::view::filler::Filler;
    use crate::view::icon::Icon;
    use crate::view::label::Label;
    use super::super::tool_bar::ToolBar;
    use super::{neighbor_book, book_label, layout, Layout, BookRow, Metrics, EndOfBook};

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
        row_height: 161,
        thickness: 2,
        padding: 38,
        label_width: 110,
        title_width: 200,
    };

    // The inside of the window, within its border.
    fn inside(rect: &Rectangle, thickness: i32) -> Rectangle {
        rect![rect.min.x + thickness, rect.min.y + thickness,
              rect.max.x - thickness, rect.max.y - thickness]
    }

    // Checks the invariants of a layout and returns it.
    fn check_layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
        let layout = layout(dims, metrics);
        let Layout { rect, library, close, title, separators, lines, books } = &layout;
        let Metrics { row_height, thickness, .. } = *metrics;
        let screen = rect![0, 0, dims.0 as i32, dims.1 as i32];

        assert!(screen.contains(rect), "{} isn't inside {}", rect, screen);
        assert!((rect.min.x - (screen.max.x - rect.max.x)).abs() <= 1);
        assert!((rect.min.y - (screen.max.y - rect.max.y)).abs() <= 1);

        // Three rows and the two lines between them fill the inside of the
        // window.
        let inner = inside(rect, thickness);
        assert_eq!(inner.height() as i32, 3 * row_height + 2 * thickness);

        // The top row: the library button at the top left, opposite the close
        // icon's square cell, each set apart by a vertical line, and the title
        // between the lines.
        assert_eq!(library.min, inner.min);
        assert_eq!(library.height() as i32, row_height);
        assert_eq!(*close, rect![inner.max.x - row_height, inner.min.y, inner.max.x, inner.min.y + row_height]);
        let [left, right] = *separators;
        assert_eq!(left, rect![library.max.x, library.min.y, library.max.x + thickness, library.max.y]);
        assert_eq!(right, rect![close.min.x - thickness, close.min.y, close.min.x, close.max.y]);
        assert!(left.max.x < right.min.x, "{} touches {}", left, right);
        if let Some(title) = title {
            assert!(left.max.x <= title.min.x && title.max.x <= right.min.x);
            assert_eq!((title.min.y, title.max.y), (close.min.y, close.max.y));
        }

        // Below it, the row of the previous book above the one of the next,
        // each under a horizontal line across the inside of the window.
        let mut min_y = close.max.y;
        for (line, book) in lines.iter().zip(books.iter()) {
            assert_eq!(*line, rect![inner.min.x, min_y, inner.max.x, min_y + thickness]);
            assert_eq!(book.rect, rect![inner.min.x, line.max.y, inner.max.x, line.max.y + row_height]);
            min_y = book.rect.max.y;
            // The chevron's square cell, a vertical line, then the title.
            let BookRow { rect: row, chevron, separator, title: book_title } = *book;
            assert_eq!(chevron, rect![row.min.x, row.min.y, row.min.x + row_height, row.max.y]);
            assert_eq!(separator, rect![chevron.max.x, row.min.y, chevron.max.x + thickness, row.max.y]);
            assert_eq!(book_title, rect![separator.max.x, row.min.y, row.max.x, row.max.y]);
        }
        assert_eq!(min_y, inner.max.y);

        // The children stay off the border and off each other.
        let mut rects = layout.children();
        rects.extend(*title);
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
        let portrait = check_layout((1072, 1448), &METRICS);
        let landscape = check_layout((1448, 1072), &METRICS);
        // The window has the same size in both orientations.
        assert_eq!(portrait.rect.width(), landscape.rect.width());
        assert_eq!(portrait.rect.height(), landscape.rect.height());
        for Layout { rect, library, title, books, .. } in [portrait, landscape] {
            assert_eq!(library.width() as i32, METRICS.label_width + 2 * METRICS.padding);
            // The title is centered on the window.
            let title = title.expect("no title");
            assert_eq!(title.min.x - rect.min.x, rect.max.x - title.max.x);
            assert!(title.width() as i32 >= METRICS.title_width + METRICS.padding);
            // The rows have room for long titles.
            for book in books {
                assert!(book.title.width() as i32 >= 2 * rect.width() as i32 / 3);
            }
        }
    }

    #[test]
    fn test_layout_top_row() {
        let Layout { library, title, close, separators, lines, .. } = check_layout((1072, 1448), &METRICS);
        // From left to right: the library button, a line, the title, a line
        // and the close icon, all above the first horizontal line.
        let cells = [library, separators[0], title.unwrap(), separators[1], close];
        for pair in cells.windows(2) {
            assert!(pair[0].max.x <= pair[1].min.x, "{} isn't left of {}", pair[0], pair[1]);
        }
        for rect in cells {
            assert_eq!(rect.max.y, lines[0].min.y);
        }
    }

    #[test]
    fn test_layout_narrow_display() {
        // The labels don't fit: the window and the library button shrink,
        // and the title goes away.
        let metrics = Metrics { label_width: 400, title_width: 300, .. METRICS };
        let Layout { rect, library, title, separators, .. } = check_layout((600, 800), &metrics);
        assert_eq!(rect.width() as i32, 600 - 2 * metrics.padding);
        assert!((library.width() as i32) < metrics.label_width + 2 * metrics.padding);
        assert!(title.is_none());
        // The vertical lines of the top row don't merge.
        assert_eq!(separators[1].min.x - separators[0].max.x, metrics.padding);
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
    fn test_rows_are_as_tall_as_the_tool_bar_ones() {
        let mut f = Fixture::new("tool-bar", (1072, 1448));
        let metrics = Metrics::new(&mut f.context.fonts);
        // The tool bar of a reflowable book, as tall as toggle_bars makes it.
        let dpi = CURRENT_DEVICE.dpi;
        let big_height = scale_by_dpi(BIG_BAR_HEIGHT, dpi) as i32;
        let thickness = scale_by_dpi(THICKNESS_MEDIUM, dpi) as i32;
        let tool_bar = ToolBar::new(rect![0, 0, 1072, 3 * big_height - thickness],
                                    true, None, &ReaderSettings::default());
        // Its first row, and the line below it.
        let (row, line) = (*tool_bar.children()[0].rect(), *tool_bar.children()[3].rect());
        assert_eq!(metrics.row_height, row.height() as i32);
        assert_eq!(metrics.thickness, line.height() as i32);
    }

    #[test]
    fn test_window_children() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("children", dims);
        let previous = info("Dune", "dune.epub");
        let mut view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let expected = layout(dims, &Metrics::new(&mut f.context.fonts));
        let title = expected.title.expect("no title");
        let mut expected_rects = expected.children();
        expected_rects.push(title);
        assert_eq!(rects(&view), expected_rects);
        assert_eq!(kinds(&view), ["button", "filler", "filler", "icon",
                                  "filler", "icon", "filler", "button",
                                  "filler", "filler", "filler", "button",
                                  "label"]);
        assert_eq!(icon(&view, 3).name, "close");

        assert!(matches!(f.tap(&mut view, expected.library.center()).as_slice(), [Event::Back]));
        assert!(matches!(f.tap(&mut view, expected.close.center()).as_slice(), [Event::Close(ViewId::EndOfBook)]));

        // The title and the lines don't respond.
        for rect in [title, expected.separators[0], expected.separators[1], expected.lines[0], expected.lines[1]] {
            assert!(f.tap(&mut view, rect.center()).is_empty(), "{} responds", rect);
        }
    }

    #[test]
    fn test_book_rows() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("rows", dims);
        let previous = info("Dune", "dune.epub");
        let next = info("", "children-of-dune.epub");
        let mut view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);
        let Layout { books, .. } = layout(dims, &Metrics::new(&mut f.context.fonts));

        // Each row starts with a chevron pointing towards its book, then a
        // line, then the title.
        assert_eq!(kinds(&view)[4..12], ["filler", "icon", "filler", "button",
                                         "filler", "icon", "filler", "button"]);
        assert_eq!(icon(&view, 5).name, "angle-left");
        assert_eq!(icon(&view, 9).name, "angle-right");
        assert!(!button(&view, 7).disabled && !button(&view, 11).disabled);

        // The chevron and the title send the same event, and so does the line
        // between them: the whole row opens the book.
        for (dir, row) in [CycleDir::Previous, CycleDir::Next].into_iter().zip(books) {
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
        let Layout { books, .. } = layout(dims, &Metrics::new(&mut f.context.fonts));

        // There's no previous book: its row stays, with an empty chevron
        // cell and a disabled title, and nothing on it responds.
        assert_eq!(kinds(&view)[5..8], ["filler", "filler", "button"]);
        assert!(button(&view, 7).disabled);
        for pt in across(&books[0].rect) {
            assert!(f.tap(&mut view, pt).is_empty(), "the tap at {:?} responds", pt);
        }

        // The row of the next book works.
        assert_eq!(icon(&view, 9).name, "angle-right");
        assert!(!button(&view, 11).disabled);
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

    #[test]
    fn test_render_plain_window() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("render", dims);
        let previous = info("Dune", "dune.epub");
        let view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let metrics = Metrics::new(&mut f.context.fonts);
        let Layout { rect, library, books, .. } = layout(dims, &metrics);
        let pixmap = render(&view, &mut f.context.fonts, dims);
        let inner = inside(&rect, metrics.thickness);

        // A black border with square corners, and nothing outside of it.
        for (pt, gray) in pixels(&pixmap, &rect![0, 0, dims.0 as i32, dims.1 as i32]) {
            if !rect.includes(pt) {
                assert_eq!(gray, WHITE.gray(), "{:?} is outside of {}", pt, rect);
            } else if !inner.includes(pt) {
                assert_eq!(gray, BLACK.gray(), "{:?} is on the border of {}", pt, rect);
            }
        }

        // The buttons are flat: the edges of their cells are white.
        for cell in [library, books[0].title, books[1].title] {
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
    }

    #[test]
    fn test_separators() {
        let dims = (1072, 1448);
        let mut f = Fixture::new("separators", dims);
        let previous = info("Dune", "dune.epub");
        let next = info("Children of Dune", "children-of-dune.epub");
        let view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);
        let metrics = Metrics::new(&mut f.context.fonts);
        let Layout { rect, separators, lines, books, .. } = layout(dims, &metrics);
        let inner = inside(&rect, metrics.thickness);
        let pixmap = render(&view, &mut f.context.fonts, dims);

        // The vertical lines of the top row and of the rows of the books,
        // between the chevrons and the titles, span the height of their row,
        // as the one next to the sync button of the home view does.
        let vertical = [(1, separators[0]), (2, separators[1]), (6, books[0].separator), (10, books[1].separator)];
        for (_, line) in vertical {
            assert_eq!((line.width() as i32, line.height() as i32), (metrics.thickness, metrics.row_height));
        }
        // The horizontal lines between the rows span the inside of the window.
        let horizontal = [(4, lines[0]), (8, lines[1])];
        for (_, line) in horizontal {
            assert_eq!((line.min.x, line.max.x), (inner.min.x, inner.max.x));
            assert_eq!(line.height() as i32, metrics.thickness);
        }

        // They're fillers of the separators' color.
        for (index, line) in vertical.into_iter().chain(horizontal) {
            assert!(view.children()[index].is::<Filler>());
            assert_eq!(*view.children()[index].rect(), line);
            for (pt, gray) in pixels(&pixmap, &line) {
                assert_eq!(gray, SEPARATOR_NORMAL.gray(), "{:?} isn't on a line", pt);
            }
        }
    }

    #[test]
    fn test_long_titles_are_truncated() {
        let mut f = Fixture::new("truncated", (1072, 1448));
        let title = "The Very Long Title of a Book That Goes On and On ".repeat(4);
        let long = info(title.trim_end(), "long.epub");
        let short = info("Dune", "dune.epub");
        let view = EndOfBook::new(Some(&long), Some(&short), &mut f.context);

        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(&mut f.context.fonts, &NORMAL_STYLE, dpi);
        let padding = font.em() as i32;
        for (index, dir, info, truncated) in [(7, CycleDir::Previous, &long, true),
                                              (11, CycleDir::Next, &short, false)] {
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

        // Nothing is drawn outside of the cell.
        let rect = *view.children()[7].rect();
        let mut pixmap = Pixmap::new(1072, 1448, 1);
        view.children()[7].render(&mut pixmap, rect, &mut f.context.fonts);
        for y in 0..pixmap.height {
            for x in 0..pixmap.width {
                if !rect.includes(pt!(x as i32, y as i32)) {
                    assert_eq!(pixmap.get_pixel(x, y), WHITE, "({}, {}) is outside of {}", x, y, rect);
                }
            }
        }
    }

    #[test]
    fn test_resize() {
        let mut f = Fixture::new("resize", (1072, 1448));
        let previous = info("Dune", "dune.epub");
        let mut view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let metrics = Metrics::new(&mut f.context.fonts);

        // Too narrow for the title, which comes back when there's room again.
        let narrow = (2 * (metrics.label_width + 2 * metrics.padding).max(metrics.row_height) + metrics.title_width) as u32;
        for (dims, has_title) in [((1448, 1072), true), ((narrow, 1072), false), ((1072, 1448), true)] {
            f.context.display.dims = dims;
            view.resize(rect![0, 0, dims.0 as i32, dims.1 as i32], &f.hub, &mut RenderQueue::new(), &mut f.context);
            let expected = layout(dims, &metrics);
            assert_eq!(view.rect, expected.rect);
            assert_eq!(expected.title.is_some(), has_title, "{:?}", dims);
            let mut expected_rects = expected.children();
            expected_rects.extend(expected.title);
            assert_eq!(rects(&view), expected_rects);
            if has_title {
                assert!(view.children().last().unwrap().is::<Label>());
            }

            // The rows keep their state.
            assert_eq!(icon(&view, 5).name, "angle-left");
            assert!(view.children()[9].is::<Filler>());
            assert!(!button(&view, 7).disabled && button(&view, 11).disabled);
            let [previous_row, next_row] = expected.books;
            assert!(all_open(&mut f, &mut view, &across(&previous_row.rect), CycleDir::Previous), "{:?}", dims);
            for pt in across(&next_row.rect) {
                assert!(f.tap(&mut view, pt).is_empty(), "the tap at {:?} responds", pt);
            }
        }
    }
}
