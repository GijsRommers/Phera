--TEST--
Ordered loose and strict search across scalar and callback-capable inputs
--FILE--
<?php
function scan($needle, $haystack, $strict) {
    return [in_array($needle, $haystack, $strict), array_search($needle, $haystack, $strict)];
}

$reference = 'return';
$object = (object) ['a' => 10];
$peer = (object) ['a' => 10];
$recursive = [1];
$recursive[] = &$recursive;
$cases = [
    ['bootTrait', ['case' => 'boottrait', 'match' => 'bootTrait', 'later' => 'bootTrait']],
    ['1e1', [0, 10, '1e1']],
    [" \t+01.0e1 ", ['first' => '10', 'later' => " \t+01.0e1 "]],
    ['.50', ['ignored' => 'plain', -3 => '0.5']],
    ['1e999', ['1e999', '2e999']],
    ["\xff", ['1', 'high' => "\xff"]],
    ["a\0b", ["a", "a\0b"]],
    [0, ['a', false, 0]],
    [null, [0, '', null]],
    ['', ['a', false, '']],
    [NAN, [NAN]],
    [INF, [INF]],
    ['return', ['reference' => &$reference]],
    [$object, [$peer, $object]],
    [[$object], [[$peer], [$object]]],
    [$recursive, [$recursive]],
    [true, []],
];
foreach ($cases as [$needle, $haystack]) {
    foreach ([false, true] as $strict) {
        echo json_encode(scan($needle, $haystack, $strict)), "\n";
    }
}
var_dump(in_array(needle: '1e1', haystack: [10]));
var_dump(array_search(needle: '10', haystack: [10], strict: 1));
foreach (['in_array', 'array_search'] as $function) {
    try { $function('x', 1); } catch (Throwable $e) { echo $e->getMessage(), "\n"; }
}
--EXPECT--
[true,"match"]
[true,"match"]
[true,1]
[true,2]
[true,"first"]
[true,"later"]
[true,-3]
[false,false]
[true,0]
[true,0]
[true,"high"]
[true,"high"]
[true,1]
[true,1]
[true,1]
[true,2]
[true,0]
[true,2]
[true,1]
[true,2]
[false,false]
[false,false]
[true,0]
[true,0]
[true,"reference"]
[true,"reference"]
[true,0]
[true,1]
[true,0]
[true,1]
[true,0]
[true,0]
[false,false]
[false,false]
bool(true)
bool(false)
in_array(): Argument #2 ($haystack) must be of type array, int given
array_search(): Argument #2 ($haystack) must be of type array, int given
