//! The sidebar's notes tree, as plain data. Port of `Views/Sidebar/NoteTree.swift`
//! and the pure helpers of `Views/Sidebar/NoteTreeState.swift`.
//!
//! The tree borrows the notes and files it groups: the store keeps them, and a
//! rebuild copies no text.

use std::cmp::Ordering;

use crate::library_list::last_path_component;
use crate::library_move::{self, Refusal};
use crate::note::{LibraryFile, Note};

/// A folder (notebook) in the tree. Folders mirror the workspace's
/// subdirectories; `id` is the folder's path relative to the workspace root
/// ("/" for the root, "/产品思考/草稿" for a nested one).
#[derive(Debug, Clone, PartialEq)]
pub struct NoteFolder<'a> {
    pub id: String,
    pub name: String,
    pub folders: Vec<NoteFolder<'a>>,
    pub notes: Vec<&'a Note>,
    /// Images, videos and PDFs, listed after the notes, by name.
    pub files: Vec<&'a LibraryFile>,
}

impl<'a> NoteFolder<'a> {
    fn new(id: String, name: String) -> Self {
        Self { id, name, folders: Vec::new(), notes: Vec::new(), files: Vec::new() }
    }

    /// Notes in this folder and every folder below it.
    pub fn total_count(&self) -> usize {
        self.notes.len() + self.folders.iter().map(NoteFolder::total_count).sum::<usize>()
    }

    /// Notes and files in this folder and every folder below it.
    pub fn item_count(&self) -> usize {
        self.notes.len() + self.files.len() + self.folders.iter().map(NoteFolder::item_count).sum::<usize>()
    }
}

/// Groups notes into a folder tree by their `relative_path`. Notes keep the
/// order they were passed in (the caller sorts them); folders and files are
/// sorted by name the way Finder does.
pub fn build<'a>(notes: &'a [Note], files: &'a [LibraryFile]) -> NoteFolder<'a> {
    let mut root = NoteFolder::new("/".to_string(), String::new());
    for note in notes {
        folder_at(&mut root, &note.relative_path).notes.push(note);
    }
    for file in files {
        folder_at(&mut root, &file.relative_path).files.push(file);
    }
    sort_folders(&mut root);
    root
}

/// The folder at `relative_path` under `root`, made as needed.
fn folder_at<'t, 'a>(root: &'t mut NoteFolder<'a>, relative_path: &str) -> &'t mut NoteFolder<'a> {
    let mut folder = root;
    for head in relative_path.split('/').filter(|part| !part.is_empty()) {
        let child_id = format!("{}/{head}", if folder.id == "/" { "" } else { &folder.id });
        let index = match folder.folders.iter().position(|child| child.id == child_id) {
            Some(index) => index,
            None => {
                folder.folders.push(NoteFolder::new(child_id, head.to_string()));
                folder.folders.len() - 1
            }
        };
        folder = &mut folder.folders[index];
    }
    folder
}

fn sort_folders(folder: &mut NoteFolder<'_>) {
    folder.folders.sort_by(|a, b| localized_standard_compare(&a.name, &b.name));
    folder.files.sort_by(|a, b| localized_standard_compare(&a.name(), &b.name()));
    for child in &mut folder.folders {
        sort_folders(child);
    }
}

/// Every folder id from the root down to (and including) the folder that holds
/// a note at `relative_path`. Used to auto-expand the path to the selected note.
pub fn ancestor_ids(relative_path: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut current = String::new();
    for component in relative_path.split('/').filter(|part| !part.is_empty()) {
        current.push('/');
        current.push_str(component);
        ids.push(current.clone());
    }
    ids
}

// MARK: - From NoteTreeState.swift

/// Every folder below `folder`, depth first in tree order: the list Move To shows.
pub fn folder_ids(folder: &NoteFolder<'_>) -> Vec<String> {
    let mut ids = Vec::new();
    for child in &folder.folders {
        ids.push(child.id.clone());
        ids.extend(folder_ids(child));
    }
    ids
}

/// The name Move To and the alerts use for a folder. `library` is the open
/// library's folder path.
pub fn folder_name(id: &str, library: Option<&str>) -> String {
    if id == "/" {
        library.map_or("Notes", last_path_component).to_string()
    } else {
        last_path_component(id).to_string()
    }
}

/// A refused move to explain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedMove {
    pub title: String,
    pub message: String,
}

/// What a refused move of the item named `item_name` into the folder named
/// `folder_name` says. `None` for the refusals that need no alert; `.intoItself`
/// beeps in the Swift app, which is the caller's to do.
pub fn failed_move(refusal: &Refusal, item_name: &str, folder_name: &str) -> Option<FailedMove> {
    match refusal {
        Refusal::NameTaken(name) => Some(FailedMove {
            title: format!("Couldn't Move “{name}”"),
            message: format!("An item named “{name}” already exists in “{folder_name}”."),
        }),
        Refusal::Failed(reason) => Some(FailedMove { title: format!("Couldn't Move “{item_name}”"), message: reason.clone() }),
        Refusal::IntoItself | Refusal::AlreadyThere | Refusal::OutsideLibrary => None,
    }
}

/// The open folders after the folder `old_id` moved to `new_id`: a moved folder
/// stays open, and so do the folders inside it.
pub fn remap_expanded(expanded: impl IntoIterator<Item = String>, old_id: &str, new_id: &str) -> Vec<String> {
    expanded.into_iter().map(|id| library_move::remap(&id, old_id, new_id).unwrap_or(id)).collect()
}

// MARK: - Finder order

/// An approximation of Foundation's `localizedStandardCompare` in an English
/// locale, the order Finder lists names in, without a collation library: runs
/// of digits compare as numbers; case and accents only break ties; spaces,
/// punctuation, symbols, digits, Latin, other alphabets, Hangul, kana and Han
/// come in that order, as in the Unicode root collation.
///
/// Not reproduced: the order a user's locale tailors (a Chinese locale sorts
/// Han by pinyin, here it's by code point), and accented letters outside Latin-1
/// and Latin Extended-A, which sort by code point after the plain Latin ones.
pub fn localized_standard_compare(a: &str, b: &str) -> Ordering {
    let (a, b) = (tokens(a), tokens(b));
    let primary = compare_by(&a, &b, Token::compare_primary);
    primary
        .then_with(|| compare_by(&a, &b, |x, y| x.accent.cmp(&y.accent)))
        .then_with(|| compare_by(&a, &b, |x, y| (x.variant, x.zeros).cmp(&(y.variant, y.zeros))))
}

fn compare_by(a: &[Token], b: &[Token], compare: impl Fn(&Token, &Token) -> Ordering) -> Ordering {
    for (x, y) in a.iter().zip(b) {
        let order = compare(x, y);
        if order != Ordering::Equal {
            return order;
        }
    }
    a.len().cmp(&b.len())
}

struct Token {
    /// The kind of character, in collation order.
    group: u8,
    /// Its order within the group.
    value: u32,
    /// A number's digits, without leading zeros.
    digits: String,
    /// The accented letter this came from, 0 for a plain one.
    accent: u32,
    /// Lowercase before uppercase, halfwidth before fullwidth, and so on.
    variant: u8,
    /// How many leading zeros a number had: `1` comes before `01`.
    zeros: usize,
}

impl Token {
    fn compare_primary(&self, other: &Self) -> Ordering {
        self.group
            .cmp(&other.group)
            .then_with(|| self.digits.len().cmp(&other.digits.len()))
            .then_with(|| self.digits.cmp(&other.digits))
            .then_with(|| self.value.cmp(&other.value))
    }
}

const GROUP_SPACE: u8 = 0;
const GROUP_PUNCTUATION: u8 = 1;
const GROUP_SYMBOL: u8 = 2;
const GROUP_DIGIT: u8 = 3;
const GROUP_LATIN: u8 = 4;
const GROUP_GREEK: u8 = 5;
const GROUP_CYRILLIC: u8 = 6;
const GROUP_OTHER_LETTER: u8 = 7;
const GROUP_HANGUL: u8 = 8;
const GROUP_KANA: u8 = 9;
const GROUP_HAN: u8 = 10;

/// ASCII punctuation and symbols in Unicode root collation order.
const ASCII_PUNCTUATION: &str = "_-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

fn tokens(text: &str) -> Vec<Token> {
    let mut out = Vec::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let (ascii, fullwidth) = match c {
            '\u{FF01}'..='\u{FF5E}' => (char::from_u32(c as u32 - 0xFEE0).unwrap_or(c), true),
            _ => (c, false),
        };
        let variant = u8::from(fullwidth) * 2;
        if ascii.is_ascii_digit() {
            let mut digits = String::from(ascii);
            while let Some(next) = chars.next_if(|next| ascii_digit(*next).is_some()) {
                digits.extend(ascii_digit(next));
            }
            let stripped = digits.trim_start_matches('0');
            // A run of only zeros is the number 0.
            let number = if stripped.is_empty() { "0" } else { stripped };
            let zeros = digits.len() - number.len();
            out.push(Token { group: GROUP_DIGIT, value: 0, digits: number.to_string(), accent: 0, variant, zeros });
        } else if let Some(base) = latin_base(ascii) {
            let upper = ascii.is_uppercase();
            for letter in base.chars() {
                out.push(Token {
                    group: GROUP_LATIN,
                    value: letter as u32,
                    digits: String::new(),
                    accent: ascii.to_lowercase().next().unwrap_or(ascii) as u32,
                    variant: variant + u8::from(upper),
                    zeros: 0,
                });
            }
        } else if ascii.is_ascii_alphabetic() {
            out.push(Token {
                group: GROUP_LATIN,
                value: ascii.to_ascii_lowercase() as u32,
                digits: String::new(),
                accent: 0,
                variant: variant + u8::from(ascii.is_ascii_uppercase()),
                zeros: 0,
            });
        } else {
            let (group, value, extra) = classify(ascii);
            out.push(Token { group, value, digits: String::new(), accent: 0, variant: variant + extra, zeros: 0 });
        }
    }
    out
}

/// The ASCII digit a digit stands for, halfwidth or fullwidth.
fn ascii_digit(c: char) -> Option<char> {
    match c {
        '0'..='9' => Some(c),
        '\u{FF10}'..='\u{FF19}' => char::from_u32(c as u32 - 0xFEE0),
        _ => None,
    }
}

/// The group and order of a character that isn't a digit or Latin letter, and
/// its variant weight.
fn classify(c: char) -> (u8, u32, u8) {
    let code = c as u32;
    if c.is_whitespace() || c.is_control() {
        return (GROUP_SPACE, code, 0);
    }
    if let Some(position) = ASCII_PUNCTUATION.find(c) {
        return (GROUP_PUNCTUATION, position as u32 * 2, 0);
    }
    // These two sort right after their ASCII counterparts.
    match c {
        '、' => return (GROUP_PUNCTUATION, ASCII_PUNCTUATION.find(',').unwrap_or(0) as u32 * 2 + 1, 0),
        '。' => return (GROUP_PUNCTUATION, ASCII_PUNCTUATION.find('.').unwrap_or(0) as u32 * 2 + 1, 0),
        _ => {}
    }
    match code {
        0x0370..=0x03FF | 0x1F00..=0x1FFF => {
            let lower = c.to_lowercase().next().unwrap_or(c);
            (GROUP_GREEK, lower as u32, u8::from(c.is_uppercase()))
        }
        0x0400..=0x052F => {
            let lower = c.to_lowercase().next().unwrap_or(c);
            (GROUP_CYRILLIC, lower as u32, u8::from(c.is_uppercase()))
        }
        0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => (GROUP_HANGUL, code, 0),
        0x3041..=0x3096 => (GROUP_KANA, code, 0),
        0x30A1..=0x30F6 => (GROUP_KANA, code - 0x60, 1),
        0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0x20000..=0x3FFFF => (GROUP_HAN, code, 0),
        // General punctuation and CJK punctuation.
        0x00A1..=0x00BF | 0x2010..=0x2027 | 0x2030..=0x205E | 0x3001..=0x303F | 0xFE30..=0xFE6F | 0xFF61..=0xFF65 => {
            (GROUP_PUNCTUATION, 0x100 + code, 0)
        }
        _ if c.is_alphabetic() => (GROUP_OTHER_LETTER, code, 0),
        _ => (GROUP_SYMBOL, code, 0),
    }
}

/// The plain Latin letters an accented letter of Latin-1 or Latin Extended-A
/// sorts as, lowercase.
fn latin_base(c: char) -> Option<&'static str> {
    Some(match c {
        'À'..='Å' | 'à'..='å' | 'Ā'..='ą' => "a",
        'Æ' | 'æ' => "ae",
        'Ç' | 'ç' | 'Ć'..='č' => "c",
        'Ď'..='đ' | 'Ð' | 'ð' => "d",
        'È'..='Ë' | 'è'..='ë' | 'Ē'..='ě' => "e",
        'Ĝ'..='ģ' => "g",
        'Ĥ'..='ħ' => "h",
        'Ì'..='Ï' | 'ì'..='ï' | 'Ĩ'..='ı' => "i",
        'Ĳ' | 'ĳ' => "ij",
        'Ĵ' | 'ĵ' => "j",
        'Ķ'..='ĸ' => "k",
        'Ĺ'..='ł' => "l",
        'Ñ' | 'ñ' | 'Ń'..='ŋ' => "n",
        'Ò'..='Ö' | 'Ø' | 'ò'..='ö' | 'ø' | 'Ō'..='ő' => "o",
        'Œ' | 'œ' => "oe",
        'Ŕ'..='ř' => "r",
        'Ś'..='š' | 'ſ' => "s",
        'ß' => "ss",
        'Ţ'..='ŧ' => "t",
        'Ù'..='Ü' | 'ù'..='ü' | 'Ũ'..='ų' => "u",
        'Ŵ' | 'ŵ' => "w",
        'Ý' | 'ý' | 'ÿ' | 'Ŷ'..='Ÿ' => "y",
        'Ź'..='ž' => "z",
        'Þ' | 'þ' => "th",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    // Ported from Tests/NoteTreeTests.swift.
    use super::*;
    use std::path::PathBuf;

    fn note(title: &str, folder: &str) -> Note {
        let dir = if folder == "/" { "" } else { folder };
        let path = format!("/ws{dir}/{title}.md");
        Note {
            id: path.clone(),
            path: PathBuf::from(path),
            title: title.to_string(),
            content: format!("# {title}\n\n"),
            tags: Vec::new(),
            created_at: 0.0,
            updated_at: 0.0,
            pinned: false,
            relative_path: folder.to_string(),
        }
    }

    fn file(name: &str, folder: &str) -> LibraryFile {
        let dir = if folder == "/" { "" } else { folder };
        let path = PathBuf::from(format!("/ws{dir}/{name}"));
        LibraryFile {
            id: path.to_string_lossy().into_owned(),
            kind: LibraryFile::kind_of(&path).unwrap(),
            path,
            updated_at: 0.0,
            relative_path: folder.to_string(),
        }
    }

    fn notes() -> Vec<Note> {
        vec![
            note("Root B", "/"),
            note("Weekly", "/产品思考"),
            note("Draft", "/产品思考/草稿"),
            note("Root A", "/"),
            note("Kyoto", "/Travel"),
            note("Slow decisions", "/产品思考"),
        ]
    }

    fn titles<'a>(folder: &NoteFolder<'a>) -> Vec<&'a str> {
        folder.notes.iter().map(|note| note.title.as_str()).collect()
    }

    fn names(folder: &NoteFolder<'_>) -> Vec<String> {
        folder.folders.iter().map(|child| child.name.clone()).collect()
    }

    fn file_names(folder: &NoteFolder<'_>) -> Vec<String> {
        folder.files.iter().map(|file| file.name()).collect()
    }

    #[test]
    fn tree_building() {
        let notes = notes();
        let root = build(&notes, &[]);

        assert_eq!(titles(&root), ["Root B", "Root A"], "root notes keep input order");
        assert_eq!(names(&root), ["Travel", "产品思考"], "folders sorted by name");
        assert_eq!(root.folders.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["/Travel", "/产品思考"], "folder ids are relative paths");
        let product = &root.folders[1];
        assert_eq!(titles(product), ["Weekly", "Slow decisions"], "folder notes keep input order");
        assert_eq!(product.folders.iter().map(|f| f.id.as_str()).collect::<Vec<_>>(), ["/产品思考/草稿"], "nested folder id");
        assert_eq!(product.total_count(), 3, "totalCount includes subfolders");
        assert_eq!(root.total_count(), 6, "root totalCount is every note");
        assert_eq!(build(&[], &[]).total_count(), 0, "empty workspace");
    }

    #[test]
    fn images_videos_pdfs() {
        let notes = notes();
        let files = [
            file("z.mp4", "/attachments"),
            file("a 10.png", "/attachments"),
            file("a 9.png", "/attachments"),
            file("cover.jpg", "/Travel"),
        ];
        let with_files = build(&notes, &files);
        assert_eq!(names(&with_files), ["attachments", "Travel", "产品思考"], "a folder holding only files is listed");
        assert_eq!(file_names(&with_files.folders[0]), ["a 9.png", "a 10.png", "z.mp4"], "files sorted like Finder");
        assert_eq!(file_names(&with_files.folders[1]), ["cover.jpg"], "file beside the notes of its folder");
        assert_eq!(titles(&with_files.folders[1]), ["Kyoto"], "notes unchanged");
        assert_eq!(with_files.total_count(), 6, "totalCount still counts notes only");
        assert_eq!(with_files.item_count(), 10, "itemCount counts files too");
    }

    #[test]
    fn ancestors() {
        assert_eq!(ancestor_ids("/"), Vec::<String>::new(), "root note has no ancestors");
        assert_eq!(ancestor_ids("/产品思考/草稿"), ["/产品思考", "/产品思考/草稿"], "ancestor chain");
    }

    // ── Beyond the Swift test ──

    #[test]
    fn folder_ids_are_depth_first() {
        let notes = notes();
        let root = build(&notes, &[]);
        assert_eq!(folder_ids(&root), ["/Travel", "/产品思考", "/产品思考/草稿"]);
    }

    #[test]
    fn names_of_folders() {
        assert_eq!(folder_name("/", Some("/w/Garden")), "Garden");
        assert_eq!(folder_name("/", None), "Notes");
        assert_eq!(folder_name("/产品思考/草稿", Some("/w/Garden")), "草稿");
    }

    #[test]
    fn refused_moves_explain_themselves() {
        let taken = failed_move(&Refusal::NameTaken("a.md".into()), "a.md", "Travel").unwrap();
        assert_eq!(taken.title, "Couldn't Move “a.md”");
        assert_eq!(taken.message, "An item named “a.md” already exists in “Travel”.");
        let failed = failed_move(&Refusal::Failed("no disk".into()), "a.md", "Travel").unwrap();
        assert_eq!(failed.title, "Couldn't Move “a.md”");
        assert_eq!(failed.message, "no disk");
        assert_eq!(failed_move(&Refusal::IntoItself, "a", "b"), None);
        assert_eq!(failed_move(&Refusal::AlreadyThere, "a", "b"), None);
        assert_eq!(failed_move(&Refusal::OutsideLibrary, "a", "b"), None);
    }

    #[test]
    fn expanded_folders_follow_a_move() {
        let expanded = vec!["/a".to_string(), "/a/b".to_string(), "/ab".to_string(), "/c".to_string()];
        assert_eq!(remap_expanded(expanded, "/a", "/x/a"), ["/x/a", "/x/a/b", "/ab", "/c"]);
    }

    #[test]
    fn finder_order_matches_foundation() {
        use Ordering::*;
        // Each expected value printed by Swift's `localizedStandardCompare`.
        for (a, b, expected) in [
            ("a 9.png", "a 10.png", Less),
            ("attachments", "Travel", Less),
            ("Travel", "产品思考", Less),
            ("a", "A", Less),
            ("A", "a", Greater),
            ("B", "a", Greater),
            ("é", "f", Less),
            ("10", "9", Greater),
            ("01", "1", Greater),
            ("1", "01", Less),
            ("a1", "a01", Less),
            ("中", "日", Less),
            ("草稿", "产品", Greater),
            ("_a", "a", Less),
            ("1", "a", Less),
            ("-b", "a", Less),
            ("a b", "ab", Less),
            ("a-b", "ab", Less),
            ("ab", "a b", Greater),
            (" a", "a", Less),
            ("😀", "a", Less),
            ("😀", "中", Less),
            ("Ａ", "a", Greater),
            ("a.b", "a b", Greater),
            ("Z", "é", Greater),
            ("ä", "z", Less),
            ("Résumé", "resume", Greater),
            ("ß", "ss", Greater),
            ("a2b", "a10b", Less),
            ("a.png", "a 2.png", Greater),
            ("a 2.png", "a.png", Less),
            ("2024", "一", Less),
            ("あ", "中", Less),
            ("가", "中", Less),
            ("あ", "a", Greater),
        ] {
            assert_eq!(localized_standard_compare(a, b), expected, "{a} vs {b}");
        }
    }
}
