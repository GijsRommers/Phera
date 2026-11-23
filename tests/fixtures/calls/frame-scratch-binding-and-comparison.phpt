--TEST--
A reference binding survives a nested comparison and return
--FILE--
<?php
function choose($left, $right, $kept) {
    if ($left == $right) { return $kept; }
    return "different";
}
function update(&$pair) {
    $local = [];
    $local[0] =& $pair[0];
    $local[0] = choose((object)['x' => 4], (object)['x' => 4], $local[0] + 1);
}
$pair = [40];
update($pair);
echo $pair[0], " ", choose([1], [2], "equal"), "\n";
--EXPECT--
41 different
