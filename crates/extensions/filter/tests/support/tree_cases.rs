//! Recursive filtering preserves PHP's writes through aliases, even after rejection.

use super::Matrix;

#[test]
fn php_array_writes_and_exception_chains() {
    let mut matrix = Matrix::default();
    matrix.expression(
        "(function () {
            $a = 'x'; $b = '22'; $c = false; $input = [&$a, &$b, [&$c]];
            try {
                filter_var($input, FILTER_VALIDATE_INT,
                    FILTER_REQUIRE_ARRAY | FILTER_THROW_ON_FAILURE);
            } catch (Filter\\FilterException $e) {
                return [$a, $b, $c, $e->getMessage(), $e->getPrevious()];
            }
        })()",
    );
    matrix.expression(
        "(function () {
            $a = 'x'; $b = new stdClass(); $input = [&$a, &$b];
            try {
                filter_var($input, FILTER_VALIDATE_INT,
                    FILTER_REQUIRE_ARRAY | FILTER_THROW_ON_FAILURE);
            } catch (Filter\\FilterException $e) {
                return [$a, $b, $e->getMessage(), $e->getPrevious()->getMessage()];
            }
        })()",
    );
    matrix.expression(
        "(function () {
            $a = 'x'; $b = 'y'; $input = [&$a, &$b];
            try {
                filter_var($input, FILTER_VALIDATE_FLOAT, [
                    'flags' => FILTER_REQUIRE_ARRAY,
                    'options' => ['decimal' => ''],
                ]);
            } catch (ValueError $e) {
                return [$a, $b, $e->getMessage(), $e->getPrevious()->getMessage()];
            }
        })()",
    );
    matrix.expression(
        "(function () {
            $a = 'x'; $b = 'y'; $input = [&$a, &$b];
            $visited = [];
            try {
                filter_var($input, FILTER_CALLBACK, [
                    'options' => function ($value) use (&$visited) {
                        $visited[] = $value;
                        throw new RuntimeException('Callback: ' . $value);
                    },
                ]);
            } catch (RuntimeException $e) {
                return [$a, $b, $visited, $e->getMessage(), $e->getPrevious()];
            }
        })()",
    );
    matrix.run("filter-tree-errors", "");
}
