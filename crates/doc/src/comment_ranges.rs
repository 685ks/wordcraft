//! Comment ranges: a [`InlineObject::CommentStart`] … [`InlineObject::CommentEnd`] pair marks
//! the text a comment is about. Unlike range fields, a comment's range may cross paragraphs and
//! table cells, so the markers are matched in story order, not per block list.
//!
//! Editing can leave a comment with only one marker (typing over or deleting a selection that
//! holds the start but not the end), or with two copies of one (copying commented text and
//! pasting it). Word rejects a file whose `w:commentRangeStart` and `w:commentRangeEnd` don't
//! pair up, so writers call [`Document::balance_comment_ranges`] first: every comment then has
//! exactly one start followed by one end in each story it is anchored in.

use std::collections::HashSet;

use crate::para::OBJ;
use crate::{Block, Blocks, CharProps, Document, InlineObject, Path, StoryRef};

/// Deepest table nesting scanned (matches `walk` in the crate root).
const MAX_DEPTH: usize = 16;

/// One comment marker: where it is, which comment, start or end.
struct Marker {
    path: Path,
    off: usize,
    id: u32,
    start: bool,
}

/// A change that balances a story's comment markers.
#[derive(Debug, PartialEq)]
enum Fix {
    /// Remove the marker at this offset (a second copy).
    Remove,
    /// Insert a marker for this comment here: start (true) or end (false).
    Insert(u32, bool),
}

fn collect(bl: &Blocks, prefix: &mut Vec<u32>, depth: usize, out: &mut Vec<Marker>) {
    if depth > MAX_DEPTH {
        return;
    }
    for (i, b) in bl.iter().enumerate() {
        prefix.push(u32::try_from(i).unwrap_or(u32::MAX));
        match &**b {
            Block::Para(p) => {
                for off in p.object_offsets() {
                    let (id, start) = match p.object_at(off) {
                        Some(InlineObject::CommentStart { id }) => (*id, true),
                        Some(InlineObject::CommentEnd { id }) => (*id, false),
                        _ => continue,
                    };
                    out.push(Marker { path: Path(prefix.clone()), off, id, start });
                }
            }
            Block::Table(t) => {
                for (r, row) in t.rows.iter().enumerate() {
                    for (c, cell) in row.cells.iter().enumerate() {
                        prefix.push(u32::try_from(r).unwrap_or(u32::MAX));
                        prefix.push(u32::try_from(c).unwrap_or(u32::MAX));
                        collect(&cell.blocks, prefix, depth + 1, out);
                        prefix.pop();
                        prefix.pop();
                    }
                }
            }
        }
        prefix.pop();
    }
}

/// The changes that make a story's markers pair up, as (paragraph, offset, fix). `seen_*` carry
/// the comments already anchored in earlier stories: a comment is anchored once.
fn plan(markers: Vec<Marker>, seen_start: &mut HashSet<u32>, seen_end: &mut HashSet<u32>) -> Vec<(Path, usize, Fix)> {
    let mut fixes = Vec::new();
    // Comments started in this story (with where), to close any still open at its end.
    let mut opened: Vec<(u32, Path, usize)> = Vec::new();
    for m in markers {
        if m.start {
            if seen_start.insert(m.id) {
                opened.push((m.id, m.path, m.off));
            } else {
                fixes.push((m.path, m.off, Fix::Remove));
            }
        } else if seen_end.contains(&m.id) {
            fixes.push((m.path, m.off, Fix::Remove));
        } else {
            // An end with no start before it: the range becomes the point at the end.
            if seen_start.insert(m.id) {
                fixes.push((m.path.clone(), m.off, Fix::Insert(m.id, true)));
            }
            seen_end.insert(m.id);
        }
    }
    // A start with no end after it: the range becomes the point at the start.
    for (id, path, off) in opened {
        if seen_end.insert(id) {
            fixes.push((path, off + OBJ.len_utf8(), Fix::Insert(id, false)));
        }
    }
    fixes
}

impl Document {
    fn comment_stories(&self) -> Vec<StoryRef> {
        std::iter::once(StoryRef::Body).chain(self.parts.keys().map(|k| StoryRef::Part(*k))).collect()
    }

    /// Every story's fixes, for the comments the document has (markers of other comments are
    /// never written).
    fn comment_fixes(&self) -> Vec<(StoryRef, Path, usize, Fix)> {
        let (mut seen_start, mut seen_end) = (HashSet::new(), HashSet::new());
        let mut out = Vec::new();
        for s in self.comment_stories() {
            let Some(bl) = self.story(s) else { continue };
            let mut markers = Vec::new();
            collect(bl, &mut Vec::new(), 0, &mut markers);
            markers.retain(|m| self.comments.contains_key(&m.id));
            out.extend(plan(markers, &mut seen_start, &mut seen_end).into_iter().map(|(p, o, f)| (s, p, o, f)));
        }
        out
    }

    /// Whether any comment lacks its start or end marker, or has a marker twice.
    pub fn has_unbalanced_comment_ranges(&self) -> bool {
        !self.comment_fixes().is_empty()
    }

    /// Make every comment's markers pair up: a missing start is added just before the end, a
    /// missing end just after the start, and second copies are removed. Returns how many
    /// markers were added or removed.
    pub fn balance_comment_ranges(&mut self) -> usize {
        let mut fixes = self.comment_fixes();
        // Last first, so earlier offsets in the same paragraph stay valid; at one offset, the
        // removal (of the marker there) before an insertion (which goes in front of it).
        fixes.sort_by(|a, b| (a.0, &a.1, a.2).cmp(&(b.0, &b.1, b.2)).then_with(|| matches!(a.3, Fix::Remove).cmp(&matches!(b.3, Fix::Remove))));
        let mut changed = 0;
        for (s, path, off, fix) in fixes.into_iter().rev() {
            let Ok(p) = self.para_mut(s, &path) else { continue };
            let done = match fix {
                Fix::Remove => p.delete(off, off + OBJ.len_utf8()).is_ok(),
                Fix::Insert(id, start) => {
                    let obj = if start { InlineObject::CommentStart { id } } else { InlineObject::CommentEnd { id } };
                    let props: CharProps = p.props_at(off).clone();
                    p.insert_object(off, obj, &props).is_ok()
                }
            };
            if done {
                changed += 1;
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Comment, PartKind, para_block};

    fn with_comments(text: &str, ids: &[u32]) -> Document {
        let mut d = Document::from_text(text);
        for id in ids {
            let part = d.add_part(PartKind::Comment, vec![para_block(crate::Paragraph::new())]);
            d.comments.insert(*id, Comment { part, ..Default::default() });
        }
        d
    }

    fn put(d: &mut Document, para: usize, off: usize, obj: InlineObject) {
        d.para_mut(StoryRef::Body, &Path::top(para)).unwrap().insert_object(off, obj, &CharProps::default()).unwrap();
    }

    /// The body's comment markers in order: (id, start?).
    fn markers(d: &Document) -> Vec<(u32, bool)> {
        let mut m = Vec::new();
        collect(d.story(StoryRef::Body).unwrap(), &mut Vec::new(), 0, &mut m);
        m.into_iter().map(|m| (m.id, m.start)).collect()
    }

    #[test]
    fn whole_ranges_are_left_alone() {
        let mut d = with_comments("ab\ncd", &[1]);
        put(&mut d, 0, 1, InlineObject::CommentStart { id: 1 });
        put(&mut d, 1, 1, InlineObject::CommentEnd { id: 1 });
        assert!(!d.has_unbalanced_comment_ranges());
        assert_eq!(d.balance_comment_ranges(), 0);
        assert_eq!(markers(&d), [(1, true), (1, false)]);
    }

    #[test]
    fn an_end_without_its_start_gets_one_just_before_it() {
        // Typing over the start of a commented passage deleted its start marker.
        let mut d = with_comments("ab", &[0]);
        put(&mut d, 0, 1, InlineObject::CommentEnd { id: 0 });
        assert!(d.has_unbalanced_comment_ranges());
        assert_eq!(d.balance_comment_ranges(), 1);
        assert_eq!(markers(&d), [(0, true), (0, false)]);
        assert!(!d.has_unbalanced_comment_ranges());
        assert_eq!(d.plain_text(StoryRef::Body), "ab");
    }

    #[test]
    fn a_start_without_its_end_gets_one_just_after_it() {
        let mut d = with_comments("ab\ncd", &[4]);
        put(&mut d, 0, 0, InlineObject::CommentStart { id: 4 });
        put(&mut d, 0, OBJ.len_utf8(), InlineObject::BookmarkStart { name: "x".into() });
        assert_eq!(d.balance_comment_ranges(), 1);
        assert_eq!(markers(&d), [(4, true), (4, false)]);
        assert!(!d.has_unbalanced_comment_ranges());
    }

    #[test]
    fn pasted_copies_and_an_end_before_its_start_are_fixed() {
        let mut d = with_comments("abcdef\ngh", &[1, 2]);
        // Comment 1 copied and pasted: two starts and two ends.
        put(&mut d, 0, 0, InlineObject::CommentStart { id: 1 });
        put(&mut d, 0, 1 + OBJ.len_utf8(), InlineObject::CommentEnd { id: 1 });
        put(&mut d, 0, 2 + 2 * OBJ.len_utf8(), InlineObject::CommentStart { id: 1 });
        put(&mut d, 0, 3 + 3 * OBJ.len_utf8(), InlineObject::CommentEnd { id: 1 });
        // Comment 2's end before its start.
        put(&mut d, 1, 0, InlineObject::CommentEnd { id: 2 });
        put(&mut d, 1, 1 + OBJ.len_utf8(), InlineObject::CommentStart { id: 2 });
        // A marker of a comment the document doesn't have is no concern here.
        put(&mut d, 1, 2 + 2 * OBJ.len_utf8(), InlineObject::CommentEnd { id: 9 });
        assert!(d.has_unbalanced_comment_ranges());
        d.balance_comment_ranges();
        assert!(!d.has_unbalanced_comment_ranges());
        assert_eq!(markers(&d), [(1, true), (1, false), (2, true), (2, false), (9, false)]);
        assert_eq!(d.plain_text(StoryRef::Body), "abcdef\ngh");
    }
}
