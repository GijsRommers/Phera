--TEST--
A loop variable that something else also sees: a closure's by-reference use, a generator's frame, and the global scope read through `global` and $GLOBALS from inside the loop.
--FILE--
<?php
// A loop variable that something else also sees: a closure's by-reference
// use, a generator's frame, and the global scope read through `global` and
// $GLOBALS from inside the loop.
function viaClosure() {
    $seen = function () use (&$v) { return $v; };
    $out = [];
    foreach ([1, 2, 3] as $v) { $out[] = $seen(); }
    return $out;
}
echo json_encode(viaClosure()), "\n";
function generate() { foreach (['a' => 1, 'b' => 2] as $k => $v) { yield "$k$v"; } yield "last $k $v"; }
foreach (generate() as $item) { echo $item, ' '; }
echo "\n";
function readGlobal() { global $g; return $g; }
foreach ([10, 20] as $g) { echo readGlobal(), ' ', $GLOBALS['g'], ' '; }
echo "\n";
$r = 'start';
$alias = &$r;
foreach (['x', 'y'] as $r) { echo "$alias "; }
echo "\n";
--EXPECT--
[1,2,3]
a1 b2 last b 2 
10 10 20 20 
x y 
