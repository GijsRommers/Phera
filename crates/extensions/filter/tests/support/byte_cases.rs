//! Sanitizers operate on byte strings, including invalid UTF-8 and embedded NULs.

use phera_extension_api::Value;

use super::Matrix;

#[test]
fn php_sanitizes_all_bytes() {
    let mut matrix = Matrix::default();
    let profiles: &[(i64, &[i64])] = &[
        (514, &[0, 4 | 8 | 512]),
        (515, &[0, 4, 8, 32, 512]),
        (516, &[0, 4, 8, 16, 32, 64, 512]),
        (517, &[0]),
        (518, &[0]),
        (519, &[0]),
        (520, &[0, 4096 | 8192 | 16384]),
        (522, &[0, 128]),
        (523, &[0]),
    ];
    for byte in 0..=u8::MAX {
        for &(id, flags) in profiles {
            for &flags in flags {
                matrix.call(
                    "filter_var",
                    &[Value::from(vec![byte]), Value::Int(id), Value::Int(flags)],
                );
            }
        }
    }
    for expression in [
        "filter_var(\"&lt; &#39; &#000000000000000000000000039; &apos;\", 522)",
        "filter_var(\"&#x00000000000000000000000000000000000041; &sup1;\", 522)",
        "filter_var(\"&#xD800; &#0; &AElig; é 😀\", 522)",
        "filter_var(\"&#039; &quot; ' \\\"\" , 522, FILTER_FLAG_NO_ENCODE_QUOTES)",
        "filter_var(\"&#x10FFFF; &#x110000; &#55296; &#1114112;\", 522)",
    ] {
        matrix.expression(expression);
    }
    matrix.run("filter-bytes", "");
}

#[test]
fn php_walks_nested_arrays_without_changing_the_input() {
    let mut matrix = Matrix::default();
    matrix.expression(
        "(function () {
            $input = '7';
            for ($i = 0; $i < 4096; ++$i) { $input = [$input]; }
            $output = filter_var_array($input, FILTER_VALIDATE_INT);
            for ($i = 0; $i < 4096; ++$i) {
                $input = $input[0];
                $output = $output[0];
            }
            return [$input, $output];
        })()",
    );
    matrix.run("filter-deep-array", "");
}
