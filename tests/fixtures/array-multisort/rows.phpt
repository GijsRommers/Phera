--TEST--
array_multisort orders multiple columns stably and renumbers numeric keys
--FILE--
<?php
$rank = ['left' => 2, 'right' => 1, 10 => 2, 14 => 2];
$label = ['b', 'x', 'A', 'b'];
$payload = [0, 1, 2, 3];
next($rank);
var_dump(array_multisort($rank, SORT_ASC, $label, SORT_DESC, SORT_STRING | SORT_FLAG_CASE, $payload));
echo json_encode([$rank, $label, $payload]), "\n";
var_dump(key($rank));
$rank[] = 7;
var_dump(array_key_last($rank));
$first = [new DateTime('2021-01-01'), new DateTime('2020-01-01')];
$names = ['later', 'earlier'];
array_multisort($first, $names);
echo implode(',', $names), "\n";
--EXPECT--
bool(true)
[{"right":1,"left":2,"0":2,"1":2},["x","b","b","A"],[1,0,3,2]]
string(5) "right"
int(4)
earlier,later
