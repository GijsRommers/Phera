--TEST--
references_as_condition_operands
--FILE--
<?php
$x = 5;
$r = &$x;
if ($r === 5) echo "ref-ident\n";
if ($r < 10) echo "ref-lt\n";
$r = "abc";
if ($r === "abc") echo "ref-str\n";
if (!$r) echo "ref-falsy\n";
$arr = [1, 2, 3];
foreach ($arr as &$v) { if ($v === 2) { $v = 20; } }
unset($v);
var_dump($arr);
$a = 1; $b = &$a;
if ($b == 1) echo "alias\n";
$b = null;
if ($a === null) echo "alias-null\n";
--EXPECTF--
ref-ident
ref-lt
ref-str
array(3) {
  [0]=>
  int(1)
  [1]=>
  int(20)
  [2]=>
  int(3)
}
alias
alias-null
