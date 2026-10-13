//! The check that nothing a phone screen draws passes the edge of its window
//! (docs/DECISIONS.md D32 item 10 (b)). The screenshot example's `--fit` and
//! the tests of the screens both use it, so every screen is held to one rule.
//!
//! iced fits each child into the room its parent gives it, so a node that is
//! too wide for its place is clamped to that room and its own box does not
//! pass the edge. The words of a text that cannot break are drawn past their
//! box all the same, so the check reads the width those words need from the
//! paragraph the tree holds for the text. What is reported is what passes the
//! edge: a label squeezed inside a button, with its words still inside the
//! window, is not, since a person cannot see past the edge but can see past a
//! box.

use iced::advanced::Layout;
use iced::advanced::layout;
use iced::advanced::widget::{Id, Operation, Tree, tree};
use iced::{Element, Point, Rectangle, Size};

/// The paragraph the screens' text is laid out in, for the renderer the
/// application draws with.
type Paragraph = <iced::Renderer as iced::advanced::text::Renderer>::Paragraph;

/// Something a screen draws past the right edge of its window.
#[derive(Clone, Debug, PartialEq)]
pub struct Past {
    /// Where it lies in the window. For a text, the box starts where the text's
    /// box starts and is as wide as the words it needs.
    pub bounds: Rectangle,
    /// The content of a text whose words run past the edge.
    pub text: Option<String>,
}

/// What `root` draws past the right edge of a window `width` logical pixels
/// wide and `height` tall, as `renderer` lays it out at that size: every node
/// whose right edge passes `width` plus half a pixel, and every text whose
/// words do. Each node's position is its parent's plus its own, so the check
/// is of what is drawn.
#[must_use]
pub fn past_the_edge<'a, M: 'a>(
    mut root: Element<'a, M>,
    renderer: &iced::Renderer,
    width: f32,
    height: f32,
) -> Vec<Past> {
    let mut tree = Tree::new(root.as_widget());
    let limits = layout::Limits::new(Size::ZERO, Size::new(width, height));
    let node = root.as_widget_mut().layout(&mut tree, renderer, &limits);
    let mut past = Vec::new();
    nodes_past(&node, Point::ORIGIN, width, &mut past);
    let mut boxes = Boxes::default();
    root.as_widget_mut()
        .operate(&mut tree, Layout::new(&node), renderer, &mut boxes);
    let mut needs = Vec::new();
    text_needs(&tree, &mut needs);
    past.extend(
        squeezed(&boxes.0, &needs, width)
            .into_iter()
            .map(|(bounds, need, text)| Past {
                bounds: Rectangle::new(bounds.position(), Size::new(need, bounds.height)),
                text: Some(text),
            }),
    );
    past
}

/// Collects, under `node` at `origin`, every node whose right edge passes
/// `width` plus half a pixel.
fn nodes_past(node: &layout::Node, origin: Point, width: f32, past: &mut Vec<Past>) {
    for child in node.children() {
        let bounds = child.bounds();
        let at = Point::new(origin.x + bounds.x, origin.y + bounds.y);
        let absolute = Rectangle::new(at, bounds.size());
        if absolute.x + absolute.width > width + 0.5 {
            past.push(Past {
                bounds: absolute,
                text: None,
            });
        }
        nodes_past(child, at, width, past);
    }
}

/// Every text an operation meets, with its box.
#[derive(Default)]
struct Boxes(Vec<(String, Rectangle)>);

impl Operation for Boxes {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }

    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
        self.0.push((text.to_owned(), bounds));
    }
}

/// Every text `tree` holds a paragraph for, in the order the tree holds them,
/// with the width its widest line takes. For words that cannot break, that is
/// more than the box they were given when they are squeezed into it.
fn text_needs(tree: &Tree, out: &mut Vec<(String, f32)>) {
    if tree.tag == tree::Tag::of::<iced::widget::text::State<Paragraph>>() {
        let state = tree
            .state
            .downcast_ref::<iced::widget::text::State<Paragraph>>();
        out.push((state.content().to_owned(), state.min_width()));
    }
    for child in &tree.children {
        text_needs(child, out);
    }
}

/// The texts whose words run past the right edge of a window `width` wide, each
/// with its box, the width its words need and its content. The operation meets
/// each text with its box, and the tree holds each text's paragraph in the same
/// order, so a box is paired with the next paragraph of the same content. A
/// paragraph with no box (a checkbox's label, which its own operation does not
/// report) is passed over when a box with its content does not follow; when one
/// does, as for a checkbox labelled as a later text, the label's paragraph is
/// paired with that box and the later text is measured by the label's
/// paragraph, which can differ from its own in size or wrapping. No screen has
/// a checkbox labelled as another of its texts. A text squeezed into its box
/// whose words stay inside the window is not reported: a label inside a button
/// that is narrower than its words is drawn inside the button, and the edge is
/// what a person cannot see past.
fn squeezed(
    boxes: &[(String, Rectangle)],
    needs: &[(String, f32)],
    width: f32,
) -> Vec<(Rectangle, f32, String)> {
    let mut past = Vec::new();
    let mut next = 0;
    for (text, bounds) in boxes {
        let Some(found) = needs[next..]
            .iter()
            .position(|(content, _)| content == text)
        else {
            continue;
        };
        let need = needs[next + found].1;
        next += found + 1;
        if bounds.x + need > width + 0.5 {
            past.push((*bounds, need, text.clone()));
        }
    }
    past
}
