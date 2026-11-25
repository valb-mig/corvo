use corvo_domain::port::{Depth, Syntax};
use corvo_domain::{Lang, Symbol};
use corvo_syntax::TreeSitter;

fn flat(lang: Lang, text: &str) -> Vec<String> {
    lines(lang, text, Depth::Items)
}

fn deep(lang: Lang, text: &str) -> Vec<String> {
    lines(lang, text, Depth::Locals)
}

fn lines(lang: Lang, text: &str, depth: Depth) -> Vec<String> {
    fn push(symbols: &[Symbol], depth: usize, out: &mut Vec<String>) {
        for symbol in symbols {
            out.push(format!(
                "{}{:?} {}",
                "  ".repeat(depth),
                symbol.kind(),
                symbol.name()
            ));
            push(symbol.nested(), depth + 1, out);
        }
    }
    let mut out = Vec::new();
    push(&TreeSitter.outline(lang, text, depth), 0, &mut out);
    out
}

#[test]
fn rust_nests_items_and_names_impls() {
    let text = "
mod m {
    pub struct S { a: u8 }
    enum E { A, B(u8) }
    trait T { fn t(&self); }
    impl T for S { fn t(&self) {} }
    impl S {
        const C: u8 = 1;
        fn new() -> Self { let x = 1; fn inner() {} S { a: x } }
    }
}
static X: u8 = 0;
type A = u8;
macro_rules! m { () => {} }
";
    assert_eq!(
        flat(Lang::Rust, text),
        [
            "Module m",
            "  Struct S",
            "    Field a",
            "  Enum E",
            "    Variant A",
            "    Variant B",
            "  Interface T",
            "    Method t",
            "  Impl T for S",
            "    Method t",
            "  Impl S",
            "    Constant C",
            "    Method new",
            "      Function inner",
            "Constant X",
            "Type A",
            "Function m",
        ]
    );
}

#[test]
fn typescript_keeps_callbacks_and_skips_locals() {
    let text = "
export function f() { const g = () => 1; let x = 2; }
export default class A {
  x = 1;
  private y: number;
  constructor() {}
  m() {}
  static s = () => {};
}
interface I { a: string; m(): void }
type T = string;
enum E { A, B = 2 }
const h = function () {}, k = 3;
let v = 1;
let [p, q] = [1, 2];
namespace N { export function q() {} }
";
    assert_eq!(
        flat(Lang::TypeScript, text),
        [
            "Function f",
            "  Function g",
            "Class A",
            "  Field x",
            "  Field y",
            "  Method constructor",
            "  Method m",
            "  Method s",
            "Interface I",
            "  Field a",
            "  Method m",
            "Type T",
            "Enum E",
            "  Variant A",
            "  Variant B",
            "Function h",
            "Constant k",
            "Variable v",
            "Module N",
            "  Function q",
        ]
    );
}

#[test]
fn tsx_finds_functions_inside_hooks() {
    let text = "
export function Page() {
  useEffect(() => {
    const load = async () => {};
    load();
  }, []);
  return <div onClick={() => {}} />;
}
";
    assert_eq!(flat(Lang::Tsx, text), ["Function Page", "  Function load"]);
}

#[test]
fn javascript_reads_class_fields() {
    let text = "class A { x = 1; #p = 2; m() {} }\nfunction* g() {}\nvar v = 1;";
    assert_eq!(
        flat(Lang::JavaScript, text),
        [
            "Class A",
            "  Field x",
            "  Field #p",
            "  Method m",
            "Function g",
            "Variable v",
        ]
    );
}

#[test]
fn python_tells_methods_fields_and_constants() {
    let text = "
LIMIT = 1
name = 'x'
class A(B):
    y: int = 2
    @property
    def m(self):
        z = 1
        def inner(): pass
def f(): pass
";
    assert_eq!(
        flat(Lang::Python, text),
        [
            "Constant LIMIT",
            "Variable name",
            "Class A",
            "  Field y",
            "  Method m",
            "    Function inner",
            "Function f",
        ]
    );
}

#[test]
fn java_lists_members_and_enum_constants() {
    let text = "
package p;
public class A {
  private int x, y = 1;
  public A() {}
  void m() { int local = 1; }
  enum E { ON, OFF; void e() {} }
  interface I { void q(); }
}
";
    assert_eq!(
        flat(Lang::Java, text),
        [
            "Class A",
            "  Field x",
            "  Field y",
            "  Method A",
            "  Method m",
            "  Enum E",
            "    Variant ON",
            "    Variant OFF",
            "    Method e",
            "  Interface I",
            "    Method q",
        ]
    );
}

#[test]
fn php_reads_properties_and_cases() {
    let text = "<?php
namespace App;
class A extends B {
    const C = 1;
    private $x, $y = 2;
    public function m() {}
}
trait T { function t() {} }
enum E: string { case On = 'on'; }
function f() {}
";
    assert_eq!(
        flat(Lang::Php, text),
        [
            "Module App",
            "Class A",
            "  Constant C",
            "  Field $x",
            "  Field $y",
            "  Method m",
            "Interface T",
            "  Method t",
            "Enum E",
            "  Variant On",
            "Function f",
        ]
    );
}

#[test]
fn kotlin_reads_classes_objects_and_properties() {
    let text = "
package p

data class D(val d: Int, other: Int)

class A {
    val x = 1
    fun m() {
        val local = 1
    }
}

object O {
    fun o() {}
}

interface I {
    fun i()
}

enum class E { ON, OFF }

fun top() {}

typealias T = Int
";
    assert_eq!(
        flat(Lang::Kotlin, text),
        [
            "Class D",
            "  Field d",
            "Class A",
            "  Field x",
            "  Method m",
            "Class O",
            "  Method o",
            "Interface I",
            "  Method i",
            "Enum E",
            "  Variant ON",
            "  Variant OFF",
            "Function top",
            "Type T",
        ]
    );
}

#[test]
fn positions_point_at_the_name_in_chars() {
    let symbols = TreeSitter.outline(
        Lang::Rust,
        "// é\nconst ÉX: u8 = 1; fn  go() {}",
        Depth::Items,
    );

    let at: Vec<_> = symbols.iter().map(|s| (s.line(), s.column())).collect();
    assert_eq!(at, [(1, 6), (1, 22)]);
}

#[test]
fn unsupported_languages_have_no_outline() {
    assert!(!TreeSitter.outlines(Lang::Markdown));
    assert!(
        TreeSitter
            .outline(Lang::Plain, "fn main() {}", Depth::Locals)
            .is_empty()
    );
}

#[test]
fn rust_locals_read_patterns_closures_and_loops() {
    let text = "
struct S;
impl S {
    fn go(&self, a: u8, (b, c): (u8, u8), Point { x, y: py }: P) {
        let Some(v) = o else { return };
        let mut w = |k, j: u8| { let z = 1; };
        let w = w;
        for (i, e) in xs {}
        if let Ok(Foo::Bar(r)) = t {}
        const LIMIT: u8 = 3;
        fn inner(q: u8) { let n = q; }
    }
}
";
    assert_eq!(
        deep(Lang::Rust, text),
        [
            "Struct S",
            "Impl S",
            "  Method go",
            "    Parameter a",
            "    Parameter b",
            "    Parameter c",
            "    Parameter x",
            "    Parameter py",
            "    Variable v",
            "    Variable w",
            "    Parameter k",
            "    Parameter j",
            "    Variable z",
            "    Variable i",
            "    Variable e",
            "    Variable r",
            "    Constant LIMIT",
            "    Function inner",
            "      Parameter q",
            "      Variable n",
        ]
    );
}

#[test]
fn locals_stay_out_of_the_items_outline() {
    let text = "fn go(a: u8) { let b = a; }";
    assert_eq!(flat(Lang::Rust, text), ["Function go"]);
    assert_eq!(
        deep(Lang::Rust, text),
        ["Function go", "  Parameter a", "  Variable b"]
    );
}

#[test]
fn typescript_locals_read_destructuring_and_callbacks() {
    let text = "
function f(a: number, b = c, {d, e: g = 1}: T, ...rest) {
  const {h, i: j} = o;
  let [k, l = 2] = arr;
  xs.map(m => m).forEach((p) => { const q = p; });
  for (const n of xs) {}
  const run = () => {};
}
";
    assert_eq!(
        deep(Lang::TypeScript, text),
        [
            "Function f",
            "  Parameter a",
            "  Parameter b",
            "  Parameter d",
            "  Parameter g",
            "  Parameter rest",
            "  Variable h",
            "  Variable j",
            "  Variable k",
            "  Variable l",
            "  Parameter m",
            "  Parameter p",
            "  Variable q",
            "  Variable n",
            "  Function run",
        ]
    );
}

#[test]
fn python_locals_skip_self_and_attributes_and_show_each_name_once() {
    let text = "
class A:
    def f(self, a, b: int, c=1, *args, **kw):
        x = 1
        y, z = 2, 3
        self.q = 4
        x = 5
        for i in xs: pass
        g = lambda u: u
";
    assert_eq!(
        deep(Lang::Python, text),
        [
            "Class A",
            "  Method f",
            "    Parameter a",
            "    Parameter b",
            "    Parameter c",
            "    Parameter args",
            "    Parameter kw",
            "    Variable x",
            "    Variable y",
            "    Variable z",
            "    Variable i",
            "    Variable g",
            "    Parameter u",
        ]
    );
}

#[test]
fn java_locals_read_declarators_loops_and_lambdas() {
    let text = "
class A {
  int field;
  void f(int a, String... b) {
    int x = 1, y;
    for (var e : xs) {}
    xs.forEach(v -> v);
    xs.forEach((p, q) -> p);
  }
}
";
    assert_eq!(
        deep(Lang::Java, text),
        [
            "Class A",
            "  Field field",
            "  Method f",
            "    Parameter a",
            "    Parameter b",
            "    Variable x",
            "    Variable y",
            "    Variable e",
            "    Parameter v",
            "    Parameter p",
            "    Parameter q",
        ]
    );
}

#[test]
fn php_locals_keep_the_dollar_and_skip_this() {
    let text = "<?php
class A {
  public function __construct(private int $id) {}
  public function f($a, int $b = 1, ...$c) {
    $x = 1;
    [$y, $z] = g();
    $this->p = 2;
    foreach ($xs as $k => $v) {}
    $fn = fn($w) => $w;
  }
}
";
    assert_eq!(
        deep(Lang::Php, text),
        [
            "Class A",
            "  Method __construct",
            "    Parameter $id",
            "  Method f",
            "    Parameter $a",
            "    Parameter $b",
            "    Parameter $c",
            "    Variable $x",
            "    Variable $y",
            "    Variable $z",
            "    Variable $k",
            "    Variable $v",
            "    Variable $fn",
            "    Parameter $w",
        ]
    );
}

#[test]
fn kotlin_locals_read_vals_loops_and_lambdas() {
    let text = "
fun f(a: Int, b: String) {
    val x: List<Int> = listOf()
    var (y, z) = p
    for (e in xs) {}
    xs.map { m -> m }
    xs.forEach { (k, v) -> k }
}
";
    assert_eq!(
        deep(Lang::Kotlin, text),
        [
            "Function f",
            "  Parameter a",
            "  Parameter b",
            "  Variable x",
            "  Variable y",
            "  Variable z",
            "  Variable e",
            "  Parameter m",
            "  Parameter k",
            "  Parameter v",
        ]
    );
}

#[test]
fn declarations_span_their_lines() {
    let text = "struct S;\nfn go(a: u8) {\n    let b = a;\n}\n";
    let symbols = TreeSitter.outline(Lang::Rust, text, Depth::Locals);

    assert_eq!((symbols[1].first_line(), symbols[1].last_line()), (1, 3));
    let local = &symbols[1].nested()[1];
    assert_eq!((local.first_line(), local.last_line()), (2, 2));
}

fn mentioned(lang: Lang, text: &str, name: &str) -> Vec<String> {
    TreeSitter
        .mentions(lang, text, name, 0..text.len())
        .into_iter()
        .map(|range| {
            let line = text[..range.start].matches('\n').count();
            format!("{line}:{}", &text[range])
        })
        .collect()
}

#[test]
fn mentions_skip_strings_comments_and_longer_names() {
    let text = "fn go(path: &str) {\n    // path here\n    let paths = \"path\";\n    open(path, path.len());\n}\n";
    assert_eq!(
        mentioned(Lang::Rust, text, "path"),
        ["0:path", "3:path", "3:path"]
    );
}

#[test]
fn mentions_keep_the_dollar_and_interpolation_in_php() {
    let text = "<?php\nfunction f($id) {\n    $x = $id + 1;\n    return \"$id\" . '$id';\n}\n";
    assert_eq!(
        mentioned(Lang::Php, text, "$id"),
        ["1:$id", "2:$id", "3:$id"]
    );
}

#[test]
fn mentions_count_interpolated_names() {
    let py = "def f(path):\n    return f\"{path}\" + \"path\"\n";
    assert_eq!(mentioned(Lang::Python, py, "path"), ["0:path", "1:path"]);
    let ts = "function f(path) { return `${path}` + 'path'; }";
    assert_eq!(
        mentioned(Lang::TypeScript, ts, "path"),
        ["0:path", "0:path"]
    );
}

#[test]
fn mentions_stay_inside_the_range() {
    let text = "fn a() { let n = 1; }\nfn b() { let n = 2; }\n";
    let second = text.find("fn b").unwrap()..text.len();
    let found = TreeSitter.mentions(Lang::Rust, text, "n", second.clone());
    assert_eq!(found.len(), 1);
    assert!(found[0].start >= second.start);
}
