--TEST--
get_error_handler() and get_exception_handler() return the callable that is installed
--FILE--
<?php
class H {
    public function __invoke($n, $m) { echo "invokable: $m\n"; return true; }
    public function method($n, $m) { echo "method: $m\n"; return true; }
    public static function stat(Throwable $e) { echo "static: ", $e->getMessage(), "\n"; }
}
function plain($n, $m) { echo "plain: $m\n"; return true; }
var_dump(get_error_handler(), get_exception_handler());
var_dump(function_exists('get_error_handler'), function_exists('get_exception_handler'));
set_error_handler('plain');
var_dump(get_error_handler());
$h = new H;
set_error_handler([$h, 'method'], E_WARNING);
var_dump(get_error_handler() === [$h, 'method']);
set_error_handler($h);
var_dump(get_error_handler() === $h);
$closure = fn($n, $m) => print("closure: $m\n");
set_error_handler($closure);
var_dump(get_error_handler() === $closure);
// what Symfony's ErrorHandler does: take the current handler, compare, put it back
$current = get_error_handler();
restore_error_handler();
var_dump(get_error_handler() === $h);
set_error_handler($current);
echo $undefined;
restore_error_handler(); restore_error_handler(); restore_error_handler();
var_dump(get_error_handler());
set_error_handler(null);
var_dump(get_error_handler());
restore_error_handler(); restore_error_handler();
var_dump(get_error_handler());
set_exception_handler('H::stat');
var_dump(get_exception_handler());
set_exception_handler(['H', 'stat']);
var_dump(get_exception_handler());
set_exception_handler(null);
var_dump(get_exception_handler());
restore_exception_handler();
var_dump(get_exception_handler());
try { get_error_handler(1); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
try { get_exception_handler(1); } catch (ArgumentCountError $e) { echo $e->getMessage(), "\n"; }
throw new Exception('handled at the end');
--EXPECT--
NULL
NULL
bool(true)
bool(true)
string(5) "plain"
bool(true)
bool(true)
bool(true)
bool(true)
closure: Undefined variable $undefined
string(5) "plain"
NULL
NULL
string(7) "H::stat"
array(2) {
  [0]=>
  string(1) "H"
  [1]=>
  string(4) "stat"
}
NULL
array(2) {
  [0]=>
  string(1) "H"
  [1]=>
  string(4) "stat"
}
get_error_handler() expects exactly 0 arguments, 1 given
get_exception_handler() expects exactly 0 arguments, 1 given
static: handled at the end
