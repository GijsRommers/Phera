--TEST--
Copying an array stops sharing an element whose last alias was unset
--FILE--
<?php
$a = ['method' => 'GET'];
$orphan =& $a['method'];
unset($orphan);
$b = $a;
$b['method'] = 1;
var_dump($a, $b);
--EXPECT--
array(1) {
  ["method"]=>
  string(3) "GET"
}
array(1) {
  ["method"]=>
  int(1)
}
