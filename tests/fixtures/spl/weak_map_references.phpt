--TEST--
WeakMap entries support indirect writes and by-reference foreach
--FILE--
<?php
$key = new stdClass();
$map = new WeakMap();
$map[$key] = [];
$map[$key][] = 4;
echo json_encode($map[$key]), "\n";
$counter = 7;
$map[$key] =& $counter;
$counter++;
echo $map->offsetGet($key), "\n";
foreach ($map as &$value) {
    $value++;
}
unset($value);
echo $counter, " ", $map[$key], "\n";
$map[$key] = 1;
echo $counter, " ", $map[$key], "\n";
$missing = new stdClass();
try {
    $map[$missing] =& $counter;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
[4]
8
9 9
9 1
Object stdClass#3 not contained in WeakMap
