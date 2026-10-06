//! Verify `**/_*` matches underscore-prefixed FILES at any depth (e.g. _DL5068-2014-meta.md)

use globset::{Glob, GlobSetBuilder};

fn main() {
    let mut builder = GlobSetBuilder::new();
    for p in ["**/_*/**", "**/_*"] {
        if let Ok(g) = Glob::new(p) {
            builder.add(g);
            println!("added pattern: {p}");
        }
    }
    let set = builder.build().unwrap();

    let cases = [
        // underscore FILES (should be excluded)
        "standards/DL5068-2014/_DL5068-2014-meta.md",
        "standards/DL5068-2014/_DL5068-2014-en-meta.md",
        "_index.md",
        "knowledge/_templates-blank.md",
        // underscore DIRS (should be excluded)
        "_trash/note.md",
        "a/_backup/x.md",
        "standards/DL5068-2014/_ocr-raw/page-001.md",
        // normal files (must NOT be excluded)
        "standards/DL5068-2014/dl5068-06.md",
        "omon4/design-basis.md",
        "knowledge/product-manuals/ch03-preface.md",
    ];
    for c in cases {
        println!("{:>2}  {c}", set.is_match(c) as i32);
    }
}
