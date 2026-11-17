--TEST--
Native classes: constant-array-key
--FILE--
<?php
$a=[PDO::FETCH_ASSOC=>"assoc",PDO::FETCH_NUM=>"num"];
 var_dump($a);

--EXPECT--
array(2) {
  [2]=>
  string(5) "assoc"
  [3]=>
  string(3) "num"
}
