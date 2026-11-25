use gpui_kit::assets::IconName as Glyph;
use gpui_kit::component::{ActiveTheme as _, Icon};
use gpui_kit::{
    App, FontWeight, Hsla, IntoElement, ParentElement as _, Pixels, RenderOnce, SharedString,
    Styled as _, Window, div, px, rgb,
};

// The file icons of edit.r: a squircle seal tinted with the tone of the file kind, holding the
// language letters, a lucide glyph or a drawn mark. Files with nothing to say get the grey leaf.

const SQUIRCLE: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M15 8L14.98 10.47L14.92 11.42L14.81 12.13L14.66 12.7L14.47 13.18L14.23 13.59L13.94 13.94L13.59 14.23L13.18 14.47L12.7 14.66L12.13 14.81L11.42 14.92L10.47 14.98L8 15L5.53 14.98L4.58 14.92L3.87 14.81L3.3 14.66L2.82 14.47L2.41 14.23L2.06 13.94L1.77 13.59L1.53 13.18L1.34 12.7L1.19 12.13L1.08 11.42L1.02 10.47L1 8L1.02 5.53L1.08 4.58L1.19 3.87L1.34 3.3L1.53 2.82L1.77 2.41L2.06 2.06L2.41 1.77L2.82 1.53L3.3 1.34L3.87 1.19L4.58 1.08L5.53 1.02L8 1L10.47 1.02L11.42 1.08L12.13 1.19L12.7 1.34L13.18 1.53L13.59 1.77L13.94 2.06L14.23 2.41L14.47 2.82L14.66 3.3L14.81 3.87L14.92 4.58L14.98 5.53Z"/></svg>"#;

const MARKDOWN: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><path d="M3.5 10.5v-5l2.25 2.5L8 5.5v5M11.25 5.5v4.75M9.6 8.75l1.65 1.65 1.65-1.65" fill="none" stroke="#000" stroke-width="1.3" stroke-linecap="round" stroke-linejoin="round"/></svg>"##;

const REACT: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"><g fill="none" stroke="#000" stroke-width="0.95"><ellipse cx="8" cy="8" rx="5.4" ry="2.1"/><ellipse cx="8" cy="8" rx="5.4" ry="2.1" transform="rotate(60 8 8)"/><ellipse cx="8" cy="8" rx="5.4" ry="2.1" transform="rotate(120 8 8)"/><circle cx="8" cy="8" r="1.05" fill="#000" stroke="none"/></g></svg>"##;

const CLAUDE: &str = "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z";

// Letters fit the seal: the more letters, the smaller, on the grid of 16.
const LETTERS: [f32; 4] = [0., 9., 7.6, 6.2];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Blue,
    Sky,
    Cyan,
    Teal,
    Green,
    Yellow,
    Orange,
    Red,
    Rose,
    Pink,
    Purple,
    Indigo,
    Claude,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Markdown,
    React,
    Claude,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Letters(&'static str, Tone),
    Glyph(Glyph, Tone),
    Mark(Mark, Tone),
    Plain(Glyph),
}

pub fn look(name: &str) -> Look {
    let lower = name.to_lowercase();
    if let Some(look) = by_name(&lower) {
        return look;
    }
    if word(&lower, &["readme"]) {
        return Look::Glyph(Glyph::BookOpen, Tone::Sky);
    }
    if word(&lower, &["license", "licence", "copying"]) {
        return Look::Plain(Glyph::Scale);
    }
    if word(&lower, &[".env"]) || lower.ends_with(".env") {
        return Look::Glyph(Glyph::KeyRound, Tone::Yellow);
    }
    if lower.starts_with("dockerfile.") || lower.ends_with(".dockerfile") {
        return Look::Glyph(Glyph::Container, Tone::Blue);
    }
    if lower.starts_with("tsconfig.") && lower.ends_with("json") {
        return Look::Glyph(Glyph::Braces, Tone::Blue);
    }
    if test(&lower) {
        return Look::Glyph(Glyph::FlaskConical, Tone::Green);
    }
    if lower.starts_with('.') && lower.ends_with("ignore") {
        return Look::Plain(Glyph::EyeOff);
    }
    match lower.rfind('.') {
        Some(dot) if dot > 0 => by_ext(&lower[dot + 1..]).unwrap_or(Look::Plain(Glyph::File)),
        _ => Look::Plain(Glyph::File),
    }
}

fn word(name: &str, words: &[&str]) -> bool {
    words.iter().any(|word| {
        name.strip_prefix(word)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

fn test(name: &str) -> bool {
    if name.ends_with("_test.go") || name.ends_with("_spec.rb") {
        return true;
    }
    if let Some(stem) = name.strip_suffix(".py") {
        return stem.starts_with("test_") || stem.ends_with("_test");
    }
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    let ext = ext.strip_prefix(['c', 'm']).unwrap_or(ext);
    matches!(ext, "js" | "jsx" | "ts" | "tsx")
        && (stem.ends_with(".test") || stem.ends_with(".spec"))
}

fn by_name(name: &str) -> Option<Look> {
    use Look::{Glyph as G, Mark as M, Plain as P};
    let look = match name {
        "claude.md" | "claude.local.md" | "agents.md" | ".mcp.json" => {
            M(Mark::Claude, Tone::Claude)
        }
        "package.json" => G(Glyph::Package, Tone::Green),
        "tsconfig.json" | "jsconfig.json" => G(Glyph::Braces, Tone::Blue),
        "dockerfile"
        | "containerfile"
        | ".dockerignore"
        | "docker-compose.yml"
        | "docker-compose.yaml"
        | "compose.yml"
        | "compose.yaml" => G(Glyph::Container, Tone::Blue),
        ".gitignore"
        | ".gitattributes"
        | ".gitmodules"
        | ".gitkeep"
        | ".git-blame-ignore-revs"
        | ".mailmap" => G(Glyph::GitBranch, Tone::Orange),
        "makefile" | "gnumakefile" | "justfile" | "cmakelists.txt" => P(Glyph::Hammer),
        ".editorconfig" | ".npmrc" | ".nvmrc" | ".yarnrc" | ".prettierrc" | ".eslintrc"
        | ".browserslistrc" | ".tool-versions" => P(Glyph::FileCog),
        "pnpm-lock.yaml" | "package-lock.json" | "yarn.lock" | "go.sum" => P(Glyph::FileLock),
        ".zshrc" | ".bashrc" | ".profile" | ".bash_profile" | ".zprofile" => {
            G(Glyph::Terminal, Tone::Green)
        }
        _ => return None,
    };
    Some(look)
}

fn by_ext(ext: &str) -> Option<Look> {
    use Look::{Glyph as G, Letters as L, Mark as M, Plain as P};
    use Tone::*;
    let look = match ext {
        "ts" | "mts" | "cts" => L("TS", Blue),
        "js" | "mjs" | "cjs" => L("JS", Yellow),
        "tsx" => M(Mark::React, Sky),
        "jsx" => M(Mark::React, Yellow),
        "py" | "pyi" | "pyw" => L("Py", Sky),
        "go" => L("Go", Cyan),
        "rs" => L("Rs", Orange),
        "java" => L("Ja", Red),
        "kt" | "kts" => L("Kt", Purple),
        "swift" => L("Sw", Rose),
        "c" => L("C", Indigo),
        "cc" | "cpp" | "cxx" | "c++" | "ixx" | "ccm" | "cppm" | "cxxm" => L("C++", Indigo),
        "h" | "hh" | "hpp" | "hxx" => L("H", Purple),
        "m" | "mm" => L("M", Indigo),
        "cs" | "csx" | "csproj" | "sln" => L("C#", Purple),
        "cu" | "cuh" => L("CU", Green),
        "fs" | "fsx" => L("F#", Sky),
        "php" => L("php", Indigo),
        "rb" | "rake" | "gemspec" => L("Rb", Red),
        "dart" => L("Dt", Teal),
        "lua" => L("Lua", Blue),
        "ex" | "exs" => L("Ex", Purple),
        "erl" | "hrl" => L("Er", Red),
        "hs" => L("Hs", Purple),
        "scala" | "sc" => L("Sc", Red),
        "clj" | "cljs" | "cljc" | "edn" => L("Cl", Green),
        "ml" | "mli" => L("ML", Orange),
        "zig" => L("Zg", Yellow),
        "nim" => L("Ni", Yellow),
        "r" => L("R", Blue),
        "jl" => L("Jl", Purple),
        "elm" => L("Elm", Sky),
        "gleam" => L("Gl", Pink),
        "sol" => L("Sol", Indigo),
        "vue" => L("V", Green),
        "svelte" => L("Sv", Orange),
        "astro" => L("As", Purple),
        "tf" | "tfvars" | "hcl" => L("Tf", Purple),
        "graphql" | "gql" => L("GQ", Pink),
        "proto" => L("Pb", Blue),
        "wasm" => L("Wa", Indigo),
        "yaml" | "yml" => L("Y", Rose),

        "json" | "json5" | "jsonc" => G(Glyph::Braces, Yellow),
        "html" | "htm" | "xhtml" => G(Glyph::Code, Orange),
        "xml" | "xsd" | "xsl" | "xslt" | "plist" => G(Glyph::CodeXml, Orange),
        "css" => G(Glyph::Hash, Blue),
        "scss" | "sass" => G(Glyph::Hash, Pink),
        "less" => G(Glyph::Hash, Indigo),
        "md" | "markdown" | "mdx" => M(Mark::Markdown, Sky),
        "sh" | "bash" | "zsh" | "fish" | "ksh" | "ps1" | "psm1" | "bat" | "cmd" => {
            G(Glyph::Terminal, Green)
        }
        "sql" => G(Glyph::Database, Indigo),
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "ico" | "avif" | "tif" | "tiff"
        | "heic" | "svg" => G(Glyph::Image, Teal),
        "mp4" | "mov" | "webm" | "mkv" | "avi" | "m4v" => G(Glyph::Clapperboard, Pink),
        "mp3" | "wav" | "ogg" | "oga" | "flac" | "m4a" | "aac" | "opus" => G(Glyph::Music, Pink),
        "csv" | "tsv" | "xls" | "xlsx" | "ods" => G(Glyph::Sheet, Green),
        "pdf" => G(Glyph::FileText, Red),
        "doc" | "docx" | "odt" | "rtf" => G(Glyph::FileText, Blue),
        "ppt" | "pptx" | "odp" => G(Glyph::Presentation, Orange),
        "ipynb" => G(Glyph::NotebookPen, Orange),
        "http" | "rest" => G(Glyph::Globe, Teal),
        "j2" | "jinja" | "jinja2" | "hbs" | "ejs" | "erb" | "twig" | "liquid" | "njk"
        | "gohtml" | "gotmpl" | "tmpl" | "razor" | "cshtml" | "heex" | "leex" | "eex" | "slim"
        | "pug" => G(Glyph::Code, Yellow),

        "txt" | "text" | "rst" | "log" => P(Glyph::FileText),
        "toml" | "ini" | "cfg" | "cnf" | "conf" | "config" | "properties" => P(Glyph::FileCog),
        "lock" | "lockb" => P(Glyph::FileLock),
        "ttf" | "otf" | "woff" | "woff2" | "eot" => P(Glyph::FileType),
        "zip" | "tar" | "gz" | "tgz" | "bz2" | "xz" | "zst" | "rar" | "7z" | "jar" | "war"
        | "apk" | "ipa" | "deb" | "rpm" | "dmg" | "iso" => P(Glyph::FileArchive),
        "pem" | "crt" | "cer" | "key" | "p12" | "pfx" | "gpg" | "asc" | "pub" => P(Glyph::FileKey),
        "db" | "sqlite" | "sqlite3" | "accdb" | "mdb" | "dbf" | "frm" | "ibd" | "ndf" | "ora" => {
            P(Glyph::Database)
        }
        "exe" | "dll" | "so" | "dylib" | "bin" | "o" | "a" | "class" | "pyc" | "beam" => {
            P(Glyph::Binary)
        }
        "cmake" => P(Glyph::Hammer),
        _ => return None,
    };
    Some(look)
}

fn color(tone: Tone) -> Hsla {
    let hex = match tone {
        Tone::Blue => 0x60a5fa,
        Tone::Sky => 0x38bdf8,
        Tone::Cyan => 0x22d3ee,
        Tone::Teal => 0x2dd4bf,
        Tone::Green => 0x34d399,
        Tone::Yellow => 0xfcd34d,
        Tone::Orange => 0xfb923c,
        Tone::Red => 0xf87171,
        Tone::Rose => 0xfb7185,
        Tone::Pink => 0xf472b6,
        Tone::Purple => 0xa78bfa,
        Tone::Indigo => 0x818cf8,
        Tone::Claude => 0xd97757,
    };
    rgb(hex).into()
}

#[derive(IntoElement)]
pub struct FileIcon {
    name: SharedString,
    size: Pixels,
}

impl FileIcon {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            size: px(16.),
        }
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for FileIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let (tone, inner) = match look(&self.name) {
            Look::Plain(glyph) => {
                return Icon::new(glyph)
                    .size(size)
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground)
                    .into_any_element();
            }
            Look::Letters(text, tone) => {
                let font = size * (LETTERS[text.chars().count().min(3)] / 16.);
                let letters = div()
                    .text_size(font)
                    .line_height(font)
                    .font_weight(FontWeight::BOLD)
                    .text_color(color(tone))
                    .child(text);
                (tone, letters.into_any_element())
            }
            Look::Glyph(glyph, tone) => {
                let glyph = Icon::new(glyph)
                    .size(size * (10. / 16.))
                    .text_color(color(tone));
                (tone, glyph.into_any_element())
            }
            Look::Mark(mark, tone) => {
                let svg = match mark {
                    Mark::Markdown => MARKDOWN,
                    Mark::React => REACT,
                    Mark::Claude => &claude("translate(3 3) scale(0.4167)", 16),
                };
                let mark = Icon::empty()
                    .data(svg.as_bytes())
                    .size(size)
                    .text_color(color(tone));
                (tone, mark.into_any_element())
            }
        };
        div()
            .relative()
            .flex_shrink_0()
            .size(size)
            .flex()
            .items_center()
            .justify_center()
            .child(
                Icon::empty()
                    .data(SQUIRCLE.as_bytes())
                    .absolute()
                    .top_0()
                    .left_0()
                    .size(size)
                    .text_color(color(tone).opacity(0.2)),
            )
            .child(inner)
            .into_any_element()
    }
}

// The lucide folders on the grid of 24, scaled to 16; known folders cut a hole in the corner for
// their glyph, like JetBrains.
const FOLDER: &str = "M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z";
const FOLDER_OPEN: &str = "m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Badge {
    Glyph(Glyph, Option<Tone>),
    Claude,
}

pub fn badge(name: &str) -> Option<Badge> {
    use Tone::*;
    let g = |glyph, tone| Some(Badge::Glyph(glyph, tone));
    match name.to_lowercase().as_str() {
        "src" | "source" | "sources" => g(Glyph::Code, Some(Blue)),
        "test" | "tests" | "__tests__" | "spec" | "specs" | "e2e" | "testdata" | "test-data"
        | "fixtures" | "__mocks__" | "mocks" => g(Glyph::FlaskConical, Some(Green)),
        "docs" | "doc" | "documentation" | "wiki" => g(Glyph::BookOpen, Some(Sky)),
        ".github" | ".gitlab" | ".git" | ".husky" | ".githooks" => {
            g(Glyph::GitBranch, Some(Orange))
        }
        "scripts" | "script" | "bin" | "cmd" | "tools" | "run" | "tasks" => {
            g(Glyph::Play, Some(Purple))
        }
        "config" | "configs" | "conf" | "settings" | ".vscode" | ".idea" | ".config"
        | ".devcontainer" => g(Glyph::Settings, None),
        "assets" | "public" | "static" | "images" | "img" | "icons" | "media" | "resources" => {
            g(Glyph::Image, Some(Teal))
        }
        "components" | "ui" | "widgets" => g(Glyph::Blocks, Some(Sky)),
        "lib" | "libs" | "packages" | "pkg" | "modules" | "vendor" | "third_party"
        | "node_modules" => g(Glyph::Package, Some(Yellow)),
        "server" | "api" | "backend" | "services" | "functions" | "lambda" => {
            g(Glyph::Server, Some(Indigo))
        }
        "web" | "www" | "frontend" | "client" | "site" => g(Glyph::Globe, Some(Sky)),
        "database" | "db" | "migrations" | "prisma" | "seeds" | "schema" | "sql" => {
            g(Glyph::Database, Some(Yellow))
        }
        "deploy" | "infra" | ".docker" | "docker" | "k8s" | "kubernetes" | "terraform" | "ops"
        | "helm" | "ansible" | ".circleci" => g(Glyph::Rocket, Some(Rose)),
        "desktop" | "electron" | "windows" | "macos" | "linux" | "src-tauri" => {
            g(Glyph::Monitor, None)
        }
        "android" => g(Glyph::Smartphone, Some(Green)),
        "mobile" | "ios" => g(Glyph::Smartphone, None),
        "styles" | "style" | "css" | "theme" | "themes" | "sass" | "scss" => {
            g(Glyph::Palette, Some(Pink))
        }
        "utils" | "util" | "helpers" => g(Glyph::Wrench, None),
        "types" | "typings" | "@types" => g(Glyph::Type, Some(Blue)),
        "locales" | "locale" | "i18n" | "lang" | "langs" | "translations" | "l10n" => {
            g(Glyph::Languages, Some(Teal))
        }
        "dist" | "build" | "out" | "target" | ".next" | ".nuxt" | ".output" | "coverage"
        | "release" => g(Glyph::Hammer, None),
        "pages" | "routes" | "views" | "screens" | "layouts" | "templates" => {
            g(Glyph::LayoutTemplate, Some(Orange))
        }
        "examples" | "example" | "demo" | "demos" | "samples" | "playground" | "sandbox" => {
            g(Glyph::Lightbulb, Some(Yellow))
        }
        ".cache" | "cache" | "tmp" | "temp" | "logs" | "log" => g(Glyph::Clock, None),
        ".claude" | "claude" => Some(Badge::Claude),
        _ => None,
    }
}

fn claude(transform: &str, grid: u8) -> String {
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {grid} {grid}"><path transform="{transform}" d="{CLAUDE}"/></svg>"#
    )
}

fn folder_svg(open: bool, cut: bool) -> String {
    let path = if open { FOLDER_OPEN } else { FOLDER };
    let folder = format!(
        r##"<path d="{path}" transform="scale(0.6667)" fill="none" stroke="#000" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/>"##
    );
    let body = if cut {
        format!(
            r##"<mask id="m"><rect width="16" height="16" fill="#fff"/><circle cx="12" cy="12" r="5.2" fill="#000"/></mask><g mask="url(#m)">{folder}</g>"##
        )
    } else {
        folder
    };
    format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16">{body}</svg>"#)
}

#[derive(IntoElement)]
pub struct FolderIcon {
    name: SharedString,
    open: bool,
    size: Pixels,
}

impl FolderIcon {
    pub fn new(name: impl Into<SharedString>) -> Self {
        Self {
            name: name.into(),
            open: false,
            size: px(16.),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    pub fn size(mut self, size: Pixels) -> Self {
        self.size = size;
        self
    }
}

impl RenderOnce for FolderIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let size = self.size;
        let muted = cx.theme().muted_foreground;
        let badge = badge(&self.name);
        let corner = |icon: Icon, color: Hsla| {
            icon.absolute()
                .left(size * (7.5 / 16.))
                .top(size * (7.5 / 16.))
                .size(size * (9. / 16.))
                .text_color(color)
        };
        div()
            .relative()
            .flex_shrink_0()
            .size(size)
            .child(
                Icon::empty()
                    .data(folder_svg(self.open, badge.is_some()).as_bytes())
                    .size(size)
                    .text_color(muted),
            )
            .children(badge.map(|badge| match badge {
                Badge::Glyph(glyph, tone) => corner(Icon::new(glyph), tone.map_or(muted, color)),
                Badge::Claude => corner(
                    Icon::empty().data(claude("", 24).as_bytes()),
                    color(Tone::Claude),
                ),
            }))
    }
}
