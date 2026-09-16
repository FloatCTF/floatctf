//! 与 floatctf-content/scripts/content.py 的 parity 交叉验证工具（开发用）。
fn main() {
    let ids = [
        "comment",
        "Android_reverse",
        "FloatCTF-qidong",
        "Cirno's perfect math class",
        "Cirno\u{2019}s book",
        "komachi's book",
        "orin's pack",
        "Flag_in_the_model",
        "foo bar",
        "foo   bar",
        "foo__bar",
        "--Foo..Bar--",
        "foo.bar",
        "题目",
        "!!!",
        "",
        "\\u5f00\\u9898",
        "a...b",
        "a___b",
        "a---b",
        "A_B",
        "Ünïcödé",
        "Café",
        "a.b_c-d",
        "题目 test",
        "_x_",
        ".y.",
        "-z-",
        "Hello World 2024",
        "n\u{0303}o",
        "ß",
        "İ",
        "9lives",
        "x",
        "a-",
        "-a",
        "a..",
        "..a",
        "a._b",
        "a-.b",
        "a . b",
    ];
    println!("=== SAFE_NAMES ===");
    for id in ids {
        println!(
            "{}\t{}",
            id.escape_debug(),
            fcmc::derive_safe_name(id).unwrap_or_default()
        );
    }
    let versions = [
        "1.0.0",
        "0.0.1",
        "12.34.56",
        "01.0.0",
        "1.0",
        "v1.0.0",
        "1.0.0-rc.1",
        "1.0.0+build",
        "1.0.0.0",
        "",
        "abc",
        "1.0.0 ",
        " 1.0.0",
        "١.٠.٠",
        "1.0.0\n",
    ];
    println!("=== VERSIONS ===");
    for v in versions {
        println!("{}\t{}", v.escape_debug(), fcmc::is_valid_version(v));
    }
}
