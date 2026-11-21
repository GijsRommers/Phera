--TEST--
array_pop then $a[] reuses the slot, and by-reference foreach still visits what PHP visits
--FILE--
<?php
function past($array) {
    end($array);
    next($array);
    array_pop($array);
    printf("%s:%s:", json_encode(key($array)), json_encode(current($array)));
    $array[] = "insert";
    printf("%s:%s\n", json_encode(key($array)), json_encode(current($array)));
}
past([1, 2, 3]);
past(["a" => 1, "b" => 2, "c" => 3]);
$array = [1, 2, 3]; unset($array[2]); past($array);
$array = [1, 2, 3]; unset($array[2], $array[1]); past($array);

foreach ([[1, 2], ['a' => 1, 'b' => 2]] as $array) {
    $seen = [];
    foreach ($array as $key => &$value) {
        $seen[] = [$key, $value];
        if ($value === 2) { array_pop($array); $array[] = 3; }
        if (count($seen) > 4) { break; }
    }
    unset($value);
    echo json_encode([$seen, $array]), "\n";
}
foreach ([[1, 2, 3], ['a'=>1, 'b'=>2, 'c'=>3]] as $array) {
    $seen = [];
    foreach ($array as $key => &$value) {
        $seen[] = [$key, $value];
        if ($value === 1) { array_pop($array); array_pop($array); $array[] = 4; }
    }
    unset($value);
    echo json_encode([$seen, $array]), "\n";
}

// A push and pop cycle keeps a list a list.
$stack = [1, 2, 3];
for ($i = 0; $i < 5; $i++) {
    $stack[] = $i;
    array_pop($stack);
}
var_dump(array_is_list($stack), $stack);
$stack[] = 'next';
echo json_encode($stack), "\n";
$map = ['a' => 1, 5 => 2];
array_pop($map);
$map[] = 'x';
echo json_encode($map), "\n";
// By value, the loop keeps its own copy.
$list = [1, 2, 3];
foreach ($list as $value) {
    echo $value;
    if ($value === 1) { array_pop($list); array_pop($list); $list[] = 9; }
}
echo ' ', json_encode($list), "\n";
// Popping elements the loop has not reached, by reference.
$list = [1, 2, 3, 4];
foreach ($list as $key => &$value) {
    echo "$key=$value ";
    if ($key === 0) { array_pop($list); $list[] = 'new'; }
}
unset($value);
echo json_encode($list), "\n";
--EXPECT--
0:1:0:1
"a":1:"a":1
0:1:0:1
null:false:3:"insert"
[[[0,1],[1,2],[1,3]],[1,3]]
[[["a",1],["b",2],[0,3]],{"a":1,"0":3}]
[[[0,1],[1,4]],[1,4]]
[[["a",1],[0,4]],{"a":1,"0":4}]
bool(true)
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(2)
  [2]=>
  int(3)
}
[1,2,3,"next"]
{"a":1,"5":"x"}
123 [1,9]
0=1 1=2 2=3 3=new [1,2,3,"new"]
