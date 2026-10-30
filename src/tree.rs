//! The tree itself: walk a folder the way Git sees it (.gitignore, .ignore and
//! Git's own excludes, through ripgrep's `ignore` crate), then write it out as a
//! tree, a Markdown block or a list of paths.
//!
//! Nothing here touches the screen or the clipboard, so it's all tested below.

use ignore::WalkBuilder;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// The walk stops after this many entries (a whole drive would take minutes).
pub const MAX_ENTRIES: usize = 100_000;

/// How the tree is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// `├── src/` lines, ready for any chat, issue or note.
    Tree,
    /// The same tree in a ```text block, for Markdown (READMEs, GitHub, Discord).
    Markdown,
    /// One relative path per line, `src/main.rs`.
    Paths,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Tree, Style::Markdown, Style::Paths];

    pub fn label(self) -> &'static str {
        match self {
            Style::Tree => "Tree",
            Style::Markdown => "Markdown",
            Style::Paths => "Paths",
        }
    }
}

/// What goes in. Mirrors the settings.
#[derive(Clone, Debug, PartialEq)]
pub struct Options {
    /// Levels below the folder; 0 = all the way down.
    pub depth: u32,
    pub folders_only: bool,
    /// Leave out what .gitignore, .ignore and Git's excludes leave out.
    pub gitignore: bool,
    /// Include dot files and files marked hidden.
    pub hidden: bool,
    /// Names always left out, wherever they are (`node_modules`, `.git`); `*` works.
    pub skip: Vec<String>,
    /// File sizes after each file.
    pub sizes: bool,
    /// At most this many entries per folder, then "… 312 more"; 0 = no limit.
    pub per_folder: usize,
    /// At most this many lines in all, then "… N more lines"; 0 = no limit.
    pub max_lines: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            depth: 0,
            folders_only: false,
            gitignore: true,
            hidden: true,
            skip: default_skip(),
            sizes: false,
            per_folder: 40,
            max_lines: 1000,
        }
    }
}

/// Folders nobody wants in a tree, Git repo or not.
pub fn default_skip() -> Vec<String> {
    [".git", "node_modules", "__pycache__", ".venv", "venv", ".idea", ".vs", ".DS_Store", "Thumbs.db", "desktop.ini"]
        .map(String::from)
        .to_vec()
}

#[derive(Clone, Debug)]
pub struct Node {
    pub name: String,
    pub dir: bool,
    pub size: u64,
    pub children: Vec<usize>,
}

/// A walked folder, ready to be written out in any style.
#[derive(Clone, Debug)]
pub struct Built {
    pub root: PathBuf,
    nodes: Vec<Node>,
    pub folders: usize,
    pub files: usize,
    /// Names from the skip list that were really there (so the card can say so).
    pub skipped: Vec<String>,
    /// The walk hit MAX_ENTRIES.
    pub cut_short: bool,
}

/// The finished text and what's in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub text: String,
    pub lines: usize,
    /// Lines left out by the line limit.
    pub lines_cut: usize,
}

fn root_name(root: &Path) -> String {
    match root.file_name() {
        Some(n) => n.to_string_lossy().into_owned(),
        // A drive: "C:\" has no file name.
        None => root.display().to_string().trim_end_matches(['\\', '/']).to_owned(),
    }
}

/// `*` matches any run of characters; everything else is literal, ignoring case.
fn glob_match(pattern: &str, name: &str) -> bool {
    let (p, n): (Vec<char>, Vec<char>) =
        (pattern.to_lowercase().chars().collect(), name.to_lowercase().chars().collect());
    let (mut pi, mut ni, mut star, mut mark) = (0, 0, None, 0);
    while ni < n.len() {
        if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ni;
            pi += 1;
        } else if pi < p.len() && p[pi] == n[ni] {
            pi += 1;
            ni += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ni = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

pub fn build(root: &Path, opts: &Options) -> std::io::Result<Built> {
    if !root.is_dir() {
        return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "that isn't a folder"));
    }
    let skip: Vec<String> = opts.skip.iter().map(|s| s.trim().to_owned()).filter(|s| !s.is_empty()).collect();
    let seen = Arc::new(Mutex::new(BTreeSet::new()));

    let mut walk = WalkBuilder::new(root);
    walk.standard_filters(false)
        .hidden(!opts.hidden)
        .git_ignore(opts.gitignore)
        .git_global(opts.gitignore)
        .git_exclude(opts.gitignore)
        .ignore(opts.gitignore)
        .parents(opts.gitignore)
        .require_git(false)
        .follow_links(false)
        .max_depth((opts.depth > 0).then_some(opts.depth as usize));
    {
        let seen = seen.clone();
        walk.filter_entry(move |e| {
            if e.depth() == 0 {
                return true;
            }
            let name = e.file_name().to_string_lossy();
            match skip.iter().find(|p| glob_match(p, &name)) {
                Some(_) => {
                    if let Ok(mut s) = seen.lock() {
                        s.insert(name.into_owned());
                    }
                    false
                }
                None => true,
            }
        });
    }

    let mut nodes = vec![Node { name: root_name(root), dir: true, size: 0, children: Vec::new() }];
    let mut index: HashMap<PathBuf, usize> = HashMap::from([(PathBuf::new(), 0)]);
    let (mut folders, mut files, mut cut_short) = (0, 0, false);
    for entry in walk.build() {
        let Ok(entry) = entry else { continue }; // unreadable folders are simply left out
        if entry.depth() == 0 {
            continue;
        }
        if folders + files >= MAX_ENTRIES {
            cut_short = true;
            break;
        }
        let dir = entry.file_type().is_some_and(|t| t.is_dir());
        if dir {
            folders += 1
        } else {
            files += 1
        }
        if !dir && opts.folders_only {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(root) else { continue };
        let Some(&parent) = index.get(rel.parent().unwrap_or(Path::new(""))) else { continue };
        let size = if dir { 0 } else { entry.metadata().map(|m| m.len()).unwrap_or(0) };
        let name = entry.file_name().to_string_lossy().into_owned();
        nodes.push(Node { name, dir, size, children: Vec::new() });
        let id = nodes.len() - 1;
        nodes[parent].children.push(id);
        if dir {
            index.insert(rel.to_path_buf(), id);
        }
    }

    // Folders first, then files, each in natural order (2 before 10).
    for i in 0..nodes.len() {
        let mut kids = std::mem::take(&mut nodes[i].children);
        kids.sort_by(|&a, &b| {
            let (a, b) = (&nodes[a], &nodes[b]);
            b.dir.cmp(&a.dir).then_with(|| natural(&a.name, &b.name))
        });
        nodes[i].children = kids;
    }
    let skipped = seen.lock().map(|s| s.iter().cloned().collect()).unwrap_or_default();
    Ok(Built { root: root.to_path_buf(), nodes, folders, files, skipped, cut_short })
}

/// Case-insensitive, with runs of digits compared as numbers.
pub fn natural(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek().copied(), y.peek().copied()) {
            (None, None) => return a.cmp(b),
            (None, _) => return Ordering::Less,
            (_, None) => return Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let take = |it: &mut std::iter::Peekable<std::str::Chars>| {
                    let mut s = String::new();
                    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
                        s.push(c);
                        it.next();
                    }
                    s
                };
                let (m, n) = (take(&mut x), take(&mut y));
                let (m, n) = (m.trim_start_matches('0'), n.trim_start_matches('0'));
                let o = m.len().cmp(&n.len()).then_with(|| m.cmp(n));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

/// "4.2 KB", "812 B", "1.3 MB".
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else if v < 10.0 {
        format!("{v:.1} {}", UNITS[u])
    } else {
        format!("{v:.0} {}", UNITS[u])
    }
}

impl Built {
    pub fn name(&self) -> &str {
        &self.nodes[0].name
    }

    /// Writes the tree out. Lines end in `\n`.
    pub fn render(&self, style: Style, opts: &Options) -> Output {
        let mut lines: Vec<String> = Vec::new();
        match style {
            Style::Tree | Style::Markdown => {
                lines.push(format!("{}/", self.name()));
                self.tree_lines(0, "", opts, &mut lines);
            }
            Style::Paths => self.path_lines(0, "", opts, &mut lines),
        }
        let mut lines_cut = 0;
        if opts.max_lines > 0 && lines.len() > opts.max_lines {
            lines_cut = lines.len() - opts.max_lines;
            lines.truncate(opts.max_lines);
            lines.push(format!("\u{2026} {lines_cut} more lines"));
        }
        if self.cut_short {
            lines.push(format!("\u{2026} stopped after {MAX_ENTRIES} entries"));
        }
        let count = lines.len();
        let mut text = lines.join("\n");
        if style == Style::Markdown {
            text = format!("```text\n{text}\n```");
        }
        text.push('\n');
        Output { text, lines: count, lines_cut }
    }

    fn label(&self, n: &Node, opts: &Options) -> String {
        if n.dir {
            format!("{}/", n.name)
        } else if opts.sizes {
            format!("{}  ({})", n.name, human_size(n.size))
        } else {
            n.name.clone()
        }
    }

    /// The children shown, and a "… N more" line for the rest.
    fn shown<'a>(&'a self, id: usize, opts: &Options) -> (&'a [usize], Option<String>) {
        let kids = &self.nodes[id].children;
        if opts.per_folder == 0 || kids.len() <= opts.per_folder {
            return (kids, None);
        }
        let rest = &kids[opts.per_folder..];
        let dirs = rest.iter().filter(|&&k| self.nodes[k].dir).count();
        let files = rest.len() - dirs;
        let what = match (dirs, files) {
            (0, f) => format!("{f} more {}", if f == 1 { "file" } else { "files" }),
            (d, 0) => format!("{d} more {}", if d == 1 { "folder" } else { "folders" }),
            (d, f) => format!("{} more ({d} folders, {f} files)", d + f),
        };
        (&kids[..opts.per_folder], Some(format!("\u{2026} {what}")))
    }

    fn tree_lines(&self, id: usize, prefix: &str, opts: &Options, out: &mut Vec<String>) {
        let (kids, more) = self.shown(id, opts);
        let count = kids.len() + usize::from(more.is_some());
        for (i, &k) in kids.iter().enumerate() {
            let last = i + 1 == count;
            let node = &self.nodes[k];
            out.push(format!(
                "{prefix}{}{}",
                if last { "\u{2514}\u{2500}\u{2500} " } else { "\u{251C}\u{2500}\u{2500} " },
                self.label(node, opts)
            ));
            if node.dir {
                let deeper = format!("{prefix}{}", if last { "    " } else { "\u{2502}   " });
                self.tree_lines(k, &deeper, opts, out);
            }
        }
        if let Some(more) = more {
            out.push(format!("{prefix}\u{2514}\u{2500}\u{2500} {more}"));
        }
    }

    fn path_lines(&self, id: usize, prefix: &str, opts: &Options, out: &mut Vec<String>) {
        let (kids, more) = self.shown(id, opts);
        for &k in kids {
            let node = &self.nodes[k];
            let path = format!("{prefix}{}", node.name);
            if node.dir {
                out.push(format!("{path}/"));
                self.path_lines(k, &format!("{path}/"), opts, out);
            } else if opts.sizes {
                out.push(format!("{path}  ({})", human_size(node.size)));
            } else {
                out.push(path);
            }
        }
        if let Some(more) = more {
            out.push(format!("{prefix}{more}"));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A small project in a temp folder: sources, a .gitignore'd build folder,
    /// node_modules, a dot file and numbered files.
    fn project(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("twig-test-{tag}-{}", std::process::id())).join("my-app");
        let _ = fs::remove_dir_all(root.parent().unwrap());
        for dir in ["src/ui", "tests", "build/out", "node_modules/left-pad", ".github/workflows"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for (file, body) in [
            ("src/main.rs", "fn main() {}"),
            ("src/ui/menu.rs", ""),
            ("src/file2.rs", ""),
            ("src/file10.rs", ""),
            ("tests/smoke.rs", ""),
            ("build/out/app.exe", "xxxxxxxx"),
            ("node_modules/left-pad/index.js", ""),
            (".github/workflows/ci.yml", ""),
            (".env", "SECRET=1"),
            (".gitignore", "build/\n.env\n"),
            ("README.md", "# hi"),
            ("Cargo.toml", ""),
        ] {
            fs::write(root.join(file), body).unwrap();
        }
        root
    }

    fn tree(root: &Path, opts: &Options) -> String {
        build(root, opts).unwrap().render(Style::Tree, opts).text
    }

    #[test]
    fn a_project_as_git_sees_it() {
        let root = project("git");
        let text = tree(&root, &Options::default());
        let want = "\
my-app/
├── .github/
│   └── workflows/
│       └── ci.yml
├── src/
│   ├── ui/
│   │   └── menu.rs
│   ├── file2.rs
│   ├── file10.rs
│   └── main.rs
├── tests/
│   └── smoke.rs
├── .gitignore
├── Cargo.toml
└── README.md
";
        assert_eq!(text, want);
        let b = build(&root, &Options::default()).unwrap();
        assert_eq!(b.skipped, vec!["node_modules".to_owned()]);
        assert_eq!((b.folders, b.files), (5, 9));
    }

    #[test]
    fn everything_when_asked() {
        let root = project("all");
        let opts = Options { gitignore: false, skip: vec![], ..Options::default() };
        let text = tree(&root, &opts);
        for name in ["build/", "app.exe", ".env", "node_modules/", "index.js"] {
            assert!(text.contains(name), "{name} missing:\n{text}");
        }
        let hidden = tree(&root, &Options { hidden: false, ..opts });
        assert!(!hidden.contains(".github") && !hidden.contains(".env") && hidden.contains("README.md"));
    }

    #[test]
    fn depth_folders_only_and_sizes() {
        let root = project("depth");
        let shallow = tree(&root, &Options { depth: 1, ..Options::default() });
        assert!(shallow.contains("├── src/\n") && !shallow.contains("main.rs"), "{shallow}");
        let dirs = tree(&root, &Options { folders_only: true, ..Options::default() });
        assert!(dirs.contains("ui/") && !dirs.contains(".rs") && !dirs.contains("README"), "{dirs}");
        let sized = tree(&root, &Options { sizes: true, ..Options::default() });
        assert!(sized.contains("main.rs  (12 B)"), "{sized}");
    }

    #[test]
    fn long_folders_and_long_trees_are_cut() {
        let root = project("long");
        let many = root.join("assets");
        fs::create_dir_all(&many).unwrap();
        for i in 0..30 {
            fs::write(many.join(format!("icon{i}.png")), "").unwrap();
        }
        let opts = Options { per_folder: 5, ..Options::default() };
        let text = tree(&root, &opts);
        assert!(text.contains("│   ├── icon4.png\n│   └── … 25 more files\n"), "{text}");
        assert!(!text.contains("icon5.png"));

        let b = build(&root, &Options::default()).unwrap();
        let out = b.render(Style::Tree, &Options { max_lines: 10, ..Options::default() });
        assert_eq!(out.lines, 11);
        assert!(out.lines_cut > 0 && out.text.ends_with(&format!("… {} more lines\n", out.lines_cut)));
    }

    #[test]
    fn markdown_and_paths() {
        let root = project("styles");
        let opts = Options::default();
        let b = build(&root, &opts).unwrap();
        let md = b.render(Style::Markdown, &opts).text;
        assert!(md.starts_with("```text\nmy-app/\n├── .github/\n") && md.ends_with("└── README.md\n```\n"), "{md}");
        let paths = b.render(Style::Paths, &opts).text;
        assert!(
            paths
                .starts_with(".github/\n.github/workflows/\n.github/workflows/ci.yml\nsrc/\nsrc/ui/\nsrc/ui/menu.rs\n")
        );
        assert!(paths.ends_with("Cargo.toml\nREADME.md\n"));
    }

    #[test]
    fn skip_patterns_and_natural_order() {
        assert!(glob_match("*.log", "Build.LOG"));
        assert!(glob_match("node_modules", "Node_Modules"));
        assert!(glob_match("cache*", "cache-v2"));
        assert!(!glob_match("*.log", "log.txt"));
        let mut names = vec!["file10", "File2", "file1", "a", "file02b"];
        names.sort_by(|a, b| natural(a, b));
        assert_eq!(names, ["a", "file1", "File2", "file02b", "file10"]);
        assert_eq!(human_size(812), "812 B");
        assert_eq!(human_size(4300), "4.2 KB");
        assert_eq!(human_size(13 * 1024 * 1024), "13 MB");
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let root = project("file");
        assert!(build(&root.join("README.md"), &Options::default()).is_err());
    }
}
