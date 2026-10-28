use std::path::{Path, PathBuf};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Framebuffer;
use crate::geom::{Rectangle, CornerSpec, BorderSpec, CycleDir};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
use crate::metadata::Info;
use crate::view::{View, Event, Hub, Bus, Id, ID_FEEDER, RenderQueue, ViewId, Align};
use crate::view::{SMALL_BAR_HEIGHT, THICKNESS_LARGE, BORDER_RADIUS_MEDIUM};
use crate::view::label::Label;
use crate::view::button::Button;
use crate::view::icon::Icon;
use crate::gesture::GestureEvent;
use crate::color::{BLACK, WHITE};
use crate::unit::scale_by_dpi;
use crate::context::Context;

const LABEL_TITLE: &str = "End of book";
const LABEL_LIBRARY: &str = "Library";

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

// The text of the line that opens the book preceding or following the
// finished one: an arrow pointing that way, then the book's title, which is
// the file name of an untitled book. The line of a missing book says so.
fn book_label(dir: CycleDir, info: Option<&Info>) -> String {
    match (dir, info) {
        (CycleDir::Previous, Some(info)) => format!("< {}", info.title()),
        (CycleDir::Next, Some(info)) => format!("> {}", info.title()),
        (CycleDir::Previous, None) => "< No previous book".to_string(),
        (CycleDir::Next, None) => "> No next book".to_string(),
    }
}

// The sizes, in pixels, the window's layout depends on.
#[derive(Debug, Copy, Clone)]
struct Metrics {
    bar_height: i32,    // Height of the top row.
    thickness: i32,     // Thickness of the window's border.
    padding: i32,
    button_height: i32,
    label_width: i32,   // Width of the library button's label.
    title_width: i32,   // Width of the title.
}

impl Metrics {
    fn new(fonts: &mut Fonts) -> Metrics {
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
        Metrics {
            bar_height: scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32,
            thickness: scale_by_dpi(THICKNESS_LARGE, dpi) as i32,
            padding: font.em() as i32,
            button_height: 4 * font.x_heights.0 as i32,
            label_width: font.plan(LABEL_LIBRARY, None, None).width,
            title_width: font.plan(LABEL_TITLE, None, None).width,
        }
    }
}

#[derive(Debug)]
struct Layout {
    rect: Rectangle,
    close: Rectangle,
    library: Rectangle,
    title: Option<Rectangle>,   // None when the title doesn't fit.
    books: [Rectangle; 2],      // The lines of the previous and next books.
}

// Centers the window on a display of the given dimensions. The top row holds
// the library button on the left, the close icon on the right, and the title,
// centered, if it fits between them. Below it, the lines of the previous and
// next books, one above the other, span the window.
fn layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
    let Metrics { bar_height, thickness, padding, button_height, label_width, title_width } = *metrics;
    let (width, height) = (dims.0 as i32, dims.1 as i32);

    // The window is as wide in both orientations, unless the top row needs
    // more room.
    let library_width = label_width + 2 * padding;
    let top_row_width = 2 * (padding + library_width).max(bar_height) + title_width + padding;
    let window_width = (5 * width.min(height) / 6).max(top_row_width)
                                                  .min(width - 2 * padding);
    let gap = padding / 2;
    let window_height = bar_height + 2 * (gap + button_height) + padding;
    let dx = (width - window_width) / 2;
    let dy = (height - window_height) / 2;
    let rect = rect![dx, dy, dx + window_width, dy + window_height];

    let close = rect![rect.max.x - bar_height, rect.min.y + thickness,
                      rect.max.x - thickness, rect.min.y + bar_height];

    // The library button shrinks when its label doesn't fit the display.
    let library_width = library_width.min(window_width - bar_height - 2 * padding);
    let min_y = rect.min.y + (thickness + bar_height - button_height) / 2;
    let library = rect![rect.min.x + padding, min_y,
                        rect.min.x + padding + library_width, min_y + button_height];

    let side = (padding + library_width).max(bar_height);
    let title = if window_width - 2 * side >= title_width + padding {
        Some(rect![rect.min.x + side, rect.min.y + thickness,
                   rect.max.x - side, rect.min.y + bar_height])
    } else {
        None
    };

    let books = [0, 1].map(|i| {
        let min_y = rect.min.y + bar_height + gap + i * (button_height + gap);
        rect![rect.min.x + padding, min_y, rect.max.x - padding, min_y + button_height]
    });

    Layout { rect, close, library, title, books }
}

// Shown when a book is finished: opens the previous or next book of the home
// view the book was opened from, or goes back to the library.
pub struct EndOfBook {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
}

impl EndOfBook {
    pub fn new(previous: Option<&Info>, next: Option<&Info>, context: &mut Context) -> EndOfBook {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let metrics = Metrics::new(&mut context.fonts);
        let Layout { rect, close, library, title, books } = layout(context.display.dims, &metrics);
        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM, CURRENT_DEVICE.dpi) as i32;

        let corners = CornerSpec::Detailed {
            north_west: 0,
            north_east: border_radius - metrics.thickness,
            south_east: 0,
            south_west: 0,
        };

        let close_icon = Icon::new("close", close, Event::Close(ViewId::EndOfBook))
                              .corners(Some(corners));
        children.push(Box::new(close_icon) as Box<dyn View>);

        let button_library = Button::new(library, Event::Back, LABEL_LIBRARY.to_string());
        children.push(Box::new(button_library) as Box<dyn View>);

        // The button draws as much of the label as fits, followed by an ellipsis.
        for ((dir, info), rect) in [(CycleDir::Previous, previous), (CycleDir::Next, next)].into_iter().zip(books) {
            let button = Button::new(rect, Event::Book(dir), book_label(dir, info))
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
        }
    }
}

impl View for EndOfBook {
    fn handle_event(&mut self, evt: &Event, hub: &Hub, _bus: &mut Bus, _rq: &mut RenderQueue, _context: &mut Context) -> bool {
        match *evt {
            Event::Gesture(GestureEvent::Tap(center)) if !self.rect.includes(center) => {
                hub.send(Event::Close(ViewId::EndOfBook)).ok();
                true
            },
            Event::Gesture(..) => true,
            _ => false,
        }
    }

    fn render(&self, fb: &mut dyn Framebuffer, _rect: Rectangle, _fonts: &mut Fonts) {
        let dpi = CURRENT_DEVICE.dpi;

        let border_radius = scale_by_dpi(BORDER_RADIUS_MEDIUM, dpi) as i32;
        let border_thickness = scale_by_dpi(THICKNESS_LARGE, dpi) as u16;

        fb.draw_rounded_rectangle_with_border(&self.rect,
                                              &CornerSpec::Uniform(border_radius),
                                              &BorderSpec { thickness: border_thickness,
                                                            color: BLACK },
                                              &WHITE);
    }

    fn resize(&mut self, _rect: Rectangle, hub: &Hub, rq: &mut RenderQueue, context: &mut Context) {
        let metrics = Metrics::new(&mut context.fonts);
        let Layout { rect, close, library, title, books } = layout(context.display.dims, &metrics);
        self.children[0].resize(close, hub, rq, context);
        self.children[1].resize(library, hub, rq, context);
        for (index, book_rect) in books.iter().enumerate() {
            self.children[index + 2].resize(*book_rect, hub, rq, context);
        }
        // The title, last, comes and goes with the room it has.
        self.children.truncate(4);
        if let Some(title) = title {
            let label = Label::new(title, LABEL_TITLE.to_string(), Align::Center);
            self.children.push(Box::new(label) as Box<dyn View>);
        }
        self.rect = rect;
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
    use crate::color::WHITE;
    use crate::context::Context;
    use crate::device::CURRENT_DEVICE;
    use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
    use crate::framebuffer::{Framebuffer, Pixmap};
    use crate::frontlight::{Frontlight, LightLevels};
    use crate::gesture::GestureEvent;
    use crate::geom::{Rectangle, CycleDir};
    use crate::library::Library;
    use crate::lightsensor::LightSensor;
    use crate::metadata::{Info, FileInfo};
    use crate::settings::{Settings, LibraryMode};
    use crate::view::{View, Event, Hub, ViewId, RenderQueue, handle_event};
    use crate::view::button::Button;
    use crate::view::icon::Icon;
    use crate::view::label::Label;
    use super::{neighbor_book, book_label, layout, Layout, Metrics, EndOfBook};

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
        let dune = info("Dune", "Fiction/dune.epub");
        assert_eq!(book_label(CycleDir::Previous, Some(&dune)), "< Dune");
        assert_eq!(book_label(CycleDir::Next, Some(&dune)), "> Dune");
    }

    #[test]
    fn test_book_label_falls_back_to_file_name() {
        let untitled = info("", "Fiction/dune-messiah.epub");
        assert_eq!(book_label(CycleDir::Previous, Some(&untitled)), "< dune-messiah");
        assert_eq!(book_label(CycleDir::Next, Some(&untitled)), "> dune-messiah");
    }

    #[test]
    fn test_book_label_missing_book() {
        assert_eq!(book_label(CycleDir::Previous, None), "< No previous book");
        assert_eq!(book_label(CycleDir::Next, None), "> No next book");
    }

    // Sizes at 300 DPI.
    const METRICS: Metrics = Metrics {
        bar_height: 121,
        thickness: 3,
        padding: 38,
        button_height: 84,
        label_width: 110,
        title_width: 200,
    };

    // Checks the invariants of a layout and returns it.
    fn check_layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
        let layout = layout(dims, metrics);
        let Layout { rect, close, library, title, books } = &layout;
        let screen = rect![0, 0, dims.0 as i32, dims.1 as i32];

        assert!(screen.contains(rect), "{} isn't inside {}", rect, screen);
        assert!((rect.min.x - (screen.max.x - rect.max.x)).abs() <= 1);
        assert!((rect.min.y - (screen.max.y - rect.max.y)).abs() <= 1);

        // The top row: the library button at the top left, opposite the close
        // icon, and the title between them.
        assert!(rect.contains(close) && rect.contains(library));
        assert_eq!(close.max.x, rect.max.x - metrics.thickness);
        assert_eq!(close.min.y, rect.min.y + metrics.thickness);
        assert_eq!(close.max.y, rect.min.y + metrics.bar_height);
        assert_eq!(library.min.x, rect.min.x + metrics.padding);
        assert!(library.max.x < close.min.x, "{} overlaps {}", library, close);
        assert!(library.min.y >= close.min.y && library.max.y <= close.max.y);
        assert!((library.center().y - close.center().y).abs() <= 1);
        assert_eq!(library.height() as i32, metrics.button_height);
        if let Some(title) = title {
            assert!(library.max.x <= title.min.x && title.max.x <= close.min.x);
            assert_eq!((title.min.y, title.max.y), (close.min.y, close.max.y));
        }

        // Below it, the line of the previous book above the one of the next.
        for book in books {
            assert!(rect.contains(book), "{} isn't inside {}", book, rect);
            assert!(book.min.y >= close.max.y);
            assert_eq!(book.height() as i32, metrics.button_height);
            assert_eq!((book.min.x, book.max.x), (rect.min.x + metrics.padding, rect.max.x - metrics.padding));
        }
        assert!(books[0].max.y < books[1].min.y, "{} overlaps {}", books[0], books[1]);

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
            assert!(library.width() as i32 >= METRICS.label_width + 2 * METRICS.padding);
            // The title is centered on the window.
            let title = title.expect("no title");
            assert_eq!(title.min.x - rect.min.x, rect.max.x - title.max.x);
            assert!(title.width() as i32 >= METRICS.title_width + METRICS.padding);
            // The lines are wide enough for long titles.
            assert!(books[0].width() as i32 >= 2 * rect.width() as i32 / 3);
        }
    }

    #[test]
    fn test_layout_top_row() {
        let Layout { library, title, close, books, .. } = check_layout((1072, 1448), &METRICS);
        let title = title.unwrap();
        // From left to right: the library button, the title, the close icon,
        // all above the lines of the books.
        assert!(library.max.x <= title.min.x && title.max.x <= close.min.x);
        for rect in [library, title, close] {
            assert!(rect.max.y <= books[0].min.y);
        }
    }

    #[test]
    fn test_layout_narrow_display() {
        // The labels don't fit: the window and the library button shrink,
        // and the title goes away.
        let metrics = Metrics { label_width: 400, title_width: 300, .. METRICS };
        let Layout { rect, library, title, .. } = check_layout((600, 800), &metrics);
        assert_eq!(rect.width() as i32, 600 - 2 * metrics.padding);
        assert!((library.width() as i32) < metrics.label_width + 2 * metrics.padding);
        assert!(title.is_none());
    }

    // A context on a display of the given dimensions, with the fonts of the
    // repository and an empty library.
    struct Fixture {
        dir: PathBuf,
        context: Context,
        hub: Hub,
        _receiver: Receiver<Event>,
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
            Fixture { dir, context, hub, _receiver: receiver }
        }

        // The events a tap at the center of `rect` sends to the window's parent.
        fn tap(&mut self, view: &mut EndOfBook, rect: Rectangle) -> Vec<Event> {
            let mut bus = VecDeque::new();
            let evt = Event::Gesture(GestureEvent::Tap(rect.center()));
            handle_event(view, &evt, &self.hub, &mut bus, &mut RenderQueue::new(), &mut self.context);
            bus.into_iter().collect()
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

    #[test]
    fn test_window_children() {
        let mut f = Fixture::new("children", (1072, 1448));
        let previous = info("Dune", "dune.epub");
        let mut view = EndOfBook::new(Some(&previous), None, &mut f.context);
        let rects = view.children().iter().map(|child| *child.rect()).collect::<Vec<Rectangle>>();
        assert_eq!(rects.len(), 5);
        let (close, library, previous_line, next_line, title) = (rects[0], rects[1], rects[2], rects[3], rects[4]);
        assert!(view.children()[0].is::<Icon>());
        assert!(view.children()[4].is::<Label>());

        // The library button at the top left, opposite the close icon.
        assert!(library.max.x <= title.min.x && title.max.x <= close.min.x);
        assert!(library.max.y <= previous_line.min.y && close.max.y <= previous_line.min.y);
        assert!(previous_line.max.y < next_line.min.y);

        assert!(matches!(f.tap(&mut view, library).as_slice(), [Event::Back]));
        assert!(matches!(f.tap(&mut view, close).as_slice(), [Event::Close(ViewId::EndOfBook)]));
        assert!(matches!(f.tap(&mut view, previous_line).as_slice(), [Event::Book(CycleDir::Previous)]));

        // There's no next book: its line is disabled.
        assert!(!button(&view, 2).disabled);
        assert!(button(&view, 3).disabled);
        assert!(f.tap(&mut view, next_line).is_empty());

        let next = info("", "children-of-dune.epub");
        let mut view = EndOfBook::new(None, Some(&next), &mut f.context);
        assert!(button(&view, 2).disabled);
        assert!(!button(&view, 3).disabled);
        let rect = *view.children()[3].rect();
        assert!(matches!(f.tap(&mut view, rect).as_slice(), [Event::Book(CycleDir::Next)]));
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
        for (index, dir, info, truncated) in [(2, CycleDir::Previous, &long, true),
                                              (3, CycleDir::Next, &short, false)] {
            let rect = *view.children()[index].rect();
            // The plan the button renders: as much of the label as fits the
            // line, followed by an ellipsis.
            let label = book_label(dir, Some(info));
            let max_width = rect.width() as i32 - padding;
            let full = font.plan(&label, None, None);
            let plan = font.plan(&label, Some(max_width), None);
            assert!(plan.width <= max_width);
            assert_eq!(full.width > max_width, truncated);
            assert_eq!(plan.width < full.width, truncated);
        }

        // Nothing is drawn outside of the line.
        let rect = *view.children()[2].rect();
        let mut pixmap = Pixmap::new(1072, 1448, 1);
        view.children()[2].render(&mut pixmap, rect, &mut f.context.fonts);
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
        let next = info("Children of Dune", "children-of-dune.epub");
        let mut view = EndOfBook::new(Some(&previous), Some(&next), &mut f.context);
        let metrics = Metrics::new(&mut f.context.fonts);

        // Too narrow for the title, which comes back when there's room again.
        let narrow = (2 * (3 * metrics.padding + metrics.label_width) + 2 * metrics.padding) as u32;
        for (dims, has_title) in [((1448, 1072), true), ((narrow, 1072), false), ((1072, 1448), true)] {
            f.context.display.dims = dims;
            view.resize(rect![0, 0, dims.0 as i32, dims.1 as i32], &f.hub, &mut RenderQueue::new(), &mut f.context);
            let expected = layout(dims, &metrics);
            assert_eq!(view.rect, expected.rect);
            assert_eq!(*view.children()[0].rect(), expected.close);
            assert_eq!(*view.children()[1].rect(), expected.library);
            assert_eq!(*view.children()[2].rect(), expected.books[0]);
            assert_eq!(*view.children()[3].rect(), expected.books[1]);
            assert_eq!(expected.title.is_some(), has_title, "{:?}", dims);
            assert_eq!(view.children().len(), if has_title { 5 } else { 4 });
            if let Some(title) = expected.title {
                assert_eq!(*view.children()[4].rect(), title);
            }
            // The lines keep their state.
            assert!(!button(&view, 2).disabled && !button(&view, 3).disabled);
        }
    }
}
