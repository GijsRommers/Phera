--TEST--
`if (condition) continue;` in every kind of loop, next to the forms that must keep their own code: a finally between the `if` and the loop, a switch, an else, `continue 2`, and conditions with side effects.
--FILE--
<?php
// `if (condition) continue;` in every kind of loop, next to the forms that
// must keep their own code: a finally between the `if` and the loop, a
// switch, an else, `continue 2`, and conditions with side effects.
function log_(string $s): bool { echo "[$s]"; return true; }
for ($i = 0; $i < 4; $i++) { if ($i % 2) continue; echo $i; }
echo "\n";
$i = 0;
while ($i < 4) { $i++; if ($i === 2 || log_("w$i") && $i === 3) continue; echo $i; }
echo "\n";
foreach ([1, 2, 3] as $v) {
    try { if ($v === 2) continue; echo $v; } finally { echo "f$v "; }
}
echo "\n";
foreach ([1, 2, 3] as $v) {
    switch ($v) { case 2: if (true) { echo "in switch "; } break; default: echo "d$v "; }
    if ($v === 1) continue;
    echo "after$v ";
}
echo "\n";
foreach ([1, 2] as $a) { foreach ([1, 2, 3] as $b) { if ($b === 2) continue 2; echo "$a$b "; } echo "never "; }
echo "\n";
foreach ([1, 2, 3] as $v) { if ($v === 2) continue; else echo "else$v "; echo "tail$v "; }
echo "\n";
$k = 0;
do { $k++; if (!($k & 1)) continue; echo "odd$k "; } while ($k < 5);
echo "\n";
foreach ([null, 0, 'x', [], [1]] as $v) { if (empty($v)) continue; if (!isset($v[0])) continue; echo json_encode($v), ' '; }
echo "\n";
function inner($items) { $out = []; foreach ($items as $item) { if ($item instanceof Countable) continue; $out[] = gettype($item); } return $out; }
echo json_encode(inner([new ArrayObject([]), 1, 'a'])), "\n";
--EXPECT--
02
[w1]1[w3][w4]4
1f1 f2 3f3 
d1 in switch after2 d3 after3 
11 21 
else1 tail1 else3 tail3 
odd1 odd3 odd5 
"x" [1] 
["integer","string"]
