--TEST--
array_multisort binds writable arguments and accepts temporary flags and arrays
--FILE--
<?php
$names = ['three', 'one', 'two'];
var_dump(array_multisort([3, 1, 2], SORT_ASC, $names));
echo implode(',', $names), "\n";
$rows = [[2, 1, 3], SORT_ASC, ['b', 'a', 'c']];
array_multisort(...$rows);
echo json_encode($rows), "\n";
$first = [2, 1, 3];
$labels = ['two', 'one', 'three'];
call_user_func_array('array_multisort', [&$first, SORT_DESC, &$labels]);
echo json_encode([$first, $labels]), "\n";
call_user_func('array_multisort', $first, SORT_ASC);
echo json_encode($first), "\n";
foreach ((new ReflectionFunction('array_multisort'))->getParameters() as $parameter) {
    var_dump($parameter->isPassedByReference(), $parameter->canBePassedByValue(), $parameter->getType());
}
--EXPECT--
bool(true)
one,two,three
[[1,2,3],4,["a","b","c"]]
[[3,2,1],["three","two","one"]]
[3,2,1]
bool(true)
bool(true)
NULL
bool(true)
bool(true)
NULL
