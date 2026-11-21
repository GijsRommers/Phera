--TEST--
Strict comparison to a literal keeps PHP value and warning semantics
--FILE--
<?php
function strict_compare($x) {
    return [$x === '123', $x !== '123', $x === "a\0b", $x !== "a\0b"];
}
foreach (['123', 123, '0', false, null, [], new stdClass(), "a\0b"] as $value) {
    echo json_encode(strict_compare($value)), "\n";
}
class Defaults {
    public const VALUE = self::INPUT === 'test';
    private const INPUT = 'test';
}
var_dump(Defaults::VALUE);
$class = new stdClass();
$class->value = '123';
$reference =& $class->value;
echo json_encode(strict_compare($reference)), "\n";
set_error_handler(function ($level, $message) { echo $message, "\n"; });
var_dump($unsetVariable === 'test');
set_error_handler(function () { throw new RuntimeException('warning interrupted comparison'); });
try {
    $result = $unsetVariable !== 'test';
} catch (RuntimeException $error) {
    echo $error->getMessage(), "\n";
}
var_dump(isset($result));
--EXPECT--
[true,false,false,true]
[false,true,false,true]
[false,true,false,true]
[false,true,false,true]
[false,true,false,true]
[false,true,false,true]
[false,true,false,true]
[false,true,true,false]
bool(true)
[true,false,false,true]
Undefined variable $unsetVariable
bool(false)
warning interrupted comparison
bool(false)
