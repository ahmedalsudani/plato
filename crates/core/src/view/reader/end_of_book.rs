use std::path::{Path, PathBuf};
use crate::device::CURRENT_DEVICE;
use crate::framebuffer::Framebuffer;
use crate::geom::{Rectangle, CornerSpec, BorderSpec, CycleDir};
use crate::font::{Fonts, font_from_style, NORMAL_STYLE};
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
const LABEL_PREVIOUS: &str = "Previous";
const LABEL_LIBRARY: &str = "Library";
const LABEL_NEXT: &str = "Next";

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

// The sizes, in pixels, the window's layout depends on.
#[derive(Debug, Copy, Clone)]
struct Metrics {
    bar_height: i32,    // Height of the title row.
    thickness: i32,     // Thickness of the window's border.
    padding: i32,
    button_height: i32,
    label_width: i32,   // Width of the widest button label.
}

impl Metrics {
    fn new(fonts: &mut Fonts) -> Metrics {
        let dpi = CURRENT_DEVICE.dpi;
        let font = font_from_style(fonts, &NORMAL_STYLE, dpi);
        let label_width = [LABEL_PREVIOUS, LABEL_LIBRARY, LABEL_NEXT].iter()
                              .map(|t| font.plan(t, None, None).width)
                              .max().unwrap();
        Metrics {
            bar_height: scale_by_dpi(SMALL_BAR_HEIGHT, dpi) as i32,
            thickness: scale_by_dpi(THICKNESS_LARGE, dpi) as i32,
            padding: font.em() as i32,
            button_height: 4 * font.x_heights.0 as i32,
            label_width,
        }
    }
}

#[derive(Debug)]
struct Layout {
    rect: Rectangle,
    close: Rectangle,
    title: Rectangle,
    buttons: [Rectangle; 3],
}

// Centers the window on a display of the given dimensions: a title row, with
// the close icon on its right, above a row of three evenly spaced buttons.
fn layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
    let Metrics { bar_height, thickness, padding, button_height, label_width } = *metrics;
    let (width, height) = (dims.0 as i32, dims.1 as i32);

    let window_width = (3 * label_width + 10 * padding).max(width / 2)
                                                       .min(width - 2 * padding);
    let window_height = 2 * bar_height + 2 * padding;
    let dx = (width - window_width) / 2;
    let dy = (height - window_height) / 2;
    let rect = rect![dx, dy, dx + window_width, dy + window_height];

    let close = rect![rect.max.x - bar_height, rect.min.y + thickness,
                      rect.max.x - thickness, rect.min.y + bar_height];
    let title = rect![rect.min.x + bar_height, rect.min.y + thickness,
                      rect.max.x - bar_height, rect.min.y + bar_height];

    // The buttons shrink when their labels don't fit the display.
    let button_width = (label_width + 2 * padding).min((window_width - 4 * padding) / 3);
    let gap = (window_width - 3 * button_width) / 4;
    let max_y = rect.min.y + 2 * bar_height;
    let min_y = max_y - button_height;
    let buttons = [0, 1, 2].map(|i| {
        let x = rect.min.x + gap + i * (button_width + gap);
        rect![x, min_y, x + button_width, max_y]
    });

    Layout { rect, close, title, buttons }
}

// Shown when a book is finished: opens the previous or next book of the home
// view the book was opened from, or goes back to the library.
pub struct EndOfBook {
    id: Id,
    rect: Rectangle,
    children: Vec<Box<dyn View>>,
}

impl EndOfBook {
    pub fn new(has_previous: bool, has_next: bool, context: &mut Context) -> EndOfBook {
        let id = ID_FEEDER.next();
        let mut children = Vec::new();
        let metrics = Metrics::new(&mut context.fonts);
        let Layout { rect, close, title, buttons } = layout(context.display.dims, &metrics);
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

        let label = Label::new(title, LABEL_TITLE.to_string(), Align::Center);
        children.push(Box::new(label) as Box<dyn View>);

        let button_previous = Button::new(buttons[0], Event::Book(CycleDir::Previous),
                                          LABEL_PREVIOUS.to_string()).disabled(!has_previous);
        children.push(Box::new(button_previous) as Box<dyn View>);

        let button_library = Button::new(buttons[1], Event::Back, LABEL_LIBRARY.to_string());
        children.push(Box::new(button_library) as Box<dyn View>);

        let button_next = Button::new(buttons[2], Event::Book(CycleDir::Next),
                                      LABEL_NEXT.to_string()).disabled(!has_next);
        children.push(Box::new(button_next) as Box<dyn View>);

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
        let Layout { rect, close, title, buttons } = layout(context.display.dims, &metrics);
        self.children[0].resize(close, hub, rq, context);
        self.children[1].resize(title, hub, rq, context);
        for (index, button_rect) in buttons.iter().enumerate() {
            self.children[index + 2].resize(*button_rect, hub, rq, context);
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
    use std::path::{Path, PathBuf};
    use crate::geom::CycleDir;
    use super::{neighbor_book, layout, Layout, Metrics};

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

    // Sizes at 300 DPI.
    const METRICS: Metrics = Metrics {
        bar_height: 121,
        thickness: 3,
        padding: 38,
        button_height: 84,
        label_width: 150,
    };

    // Checks the invariants of a layout and returns it.
    fn check_layout(dims: (u32, u32), metrics: &Metrics) -> Layout {
        let layout = layout(dims, metrics);
        let Layout { rect, close, title, buttons } = &layout;
        let screen = rect![0, 0, dims.0 as i32, dims.1 as i32];

        assert!(screen.contains(rect), "{} isn't inside {}", rect, screen);
        assert!((rect.min.x - (screen.max.x - rect.max.x)).abs() <= 1);
        assert!((rect.min.y - (screen.max.y - rect.max.y)).abs() <= 1);

        assert!(rect.contains(close) && rect.contains(title));
        assert!(title.max.x <= close.min.x);
        assert_eq!(close.max.x, rect.max.x - metrics.thickness);

        for button in buttons {
            assert!(rect.contains(button), "{} isn't inside {}", button, rect);
            assert!(button.min.y >= title.max.y);
            assert_eq!(button.height() as i32, metrics.button_height);
        }

        for pair in buttons.windows(2) {
            assert!(pair[0].max.x < pair[1].min.x, "{} overlaps {}", pair[0], pair[1]);
            assert_eq!(pair[0].min.y, pair[1].min.y);
        }

        layout
    }

    #[test]
    fn test_layout_portrait_and_landscape() {
        for dims in [(1072, 1448), (1448, 1072)] {
            let Layout { rect, buttons, .. } = check_layout(dims, &METRICS);
            for button in &buttons {
                assert!(button.width() as i32 >= METRICS.label_width + 2 * METRICS.padding);
            }
            // The buttons are evenly spaced.
            let left_gap = buttons[0].min.x - rect.min.x;
            let middle_gap = buttons[1].min.x - buttons[0].max.x;
            let right_gap = rect.max.x - buttons[2].max.x;
            assert_eq!(left_gap, middle_gap);
            assert!((right_gap - left_gap).abs() <= 3);
        }
    }

    #[test]
    fn test_layout_narrow_display() {
        // The labels don't fit: the window and its buttons shrink.
        let metrics = Metrics { label_width: 400, .. METRICS };
        let Layout { rect, .. } = check_layout((600, 800), &metrics);
        assert_eq!(rect.width() as i32, 600 - 2 * metrics.padding);
    }
}
