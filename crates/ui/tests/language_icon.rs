use corvo_ui::language_icon::{logo, tint};

#[test]
fn every_counted_language_has_its_logo() {
    for name in [
        "Rust",
        "TypeScript",
        "JavaScript",
        "Python",
        "Java",
        "Kotlin",
        "PHP",
        "Go",
        "Ruby",
        "Swift",
        "Dart",
        "C",
        "C++",
        "C#",
        "Vue",
        "Svelte",
        "Astro",
        "HTML",
        "CSS",
        "Shell",
        "Lua",
        "SQL",
        "Elixir",
        "Zig",
        "Scala",
        "Haskell",
    ] {
        let (svg, _) = logo(name).unwrap_or_else(|| panic!("{name} has no logo"));
        assert!(svg.contains("<path"), "{name}");
    }
    assert!(logo("Brainfuck").is_none());
}

#[test]
fn dark_brand_colors_are_lifted_and_bright_ones_kept() {
    assert!(tint(0x000000).l >= 0.7);
    assert!(tint(0x000080).l >= 0.7);
    let typescript = tint(0x3178C6);
    assert!(typescript.l > 0.35 && typescript.l < 0.7);
}
