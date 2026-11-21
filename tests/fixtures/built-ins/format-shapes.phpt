--TEST--
Scalar formatting and stringable arguments keep their output order
--FILE--
<?php
class Part {
    public function __toString(): string {
        echo "object\n";
        return 'part';
    }
}

function probe($format, $value) {
    var_dump(sprintf($format, $value, 'ABC'));
    var_dump(vsprintf($format, ['key' => $value, 'ABC']));
    ob_start();
    $printed = printf($format, $value, 'ABC');
    $vprinted = vprintf($format, [$value, 'ABC']);
    var_dump(ob_get_clean(), $printed, $vprinted);
}

probe('%2$-4.2s|%1$05d', -7);
probe('%2$-4.2s|%1$05d', 23);
probe('%s:%s', new Part());
var_dump(implode(',', ['x' => PHP_INT_MIN, true, false, null, 'end']));
var_dump(implode(',', ['x' => PHP_INT_MAX, true, false, null, 'end']));
var_dump(join(['a', new Part(), 'z']));

function fail_format($format) {
    ob_start();
    try {
        printf($format, 5);
    } catch (ArgumentCountError $error) {
        var_dump(ob_get_clean(), $error->getMessage());
    }
}
fail_format('%d %d');
fail_format('%d %d');
--EXPECT--
string(10) "AB  |-0007"
string(10) "AB  |-0007"
string(20) "AB  |-0007AB  |-0007"
int(10)
int(10)
string(10) "AB  |00023"
string(10) "AB  |00023"
string(20) "AB  |00023AB  |00023"
int(10)
int(10)
object
string(8) "part:ABC"
object
string(8) "part:ABC"
string(30) "object
part:ABCobject
part:ABC"
int(8)
int(8)
string(28) "-9223372036854775808,1,,,end"
string(27) "9223372036854775807,1,,,end"
object
string(6) "apartz"
string(0) ""
string(33) "3 arguments are required, 2 given"
string(0) ""
string(33) "3 arguments are required, 2 given"
