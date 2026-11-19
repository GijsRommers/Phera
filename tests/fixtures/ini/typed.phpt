--TEST--
Typed INI values follow scanner tokens
--FILE--
<?php
$ini = "a=TRUE\nb=none\nc=null\nd=12\ne=.5\nf=-2.5\ng=+10\nh=1e2";
var_dump(parse_ini_string($ini, scanner_mode: INI_SCANNER_TYPED));
--EXPECT--
array(8) {
  ["a"]=>
  bool(true)
  ["b"]=>
  bool(false)
  ["c"]=>
  NULL
  ["d"]=>
  int(12)
  ["e"]=>
  float(0.5)
  ["f"]=>
  string(4) "-2.5"
  ["g"]=>
  string(3) "+10"
  ["h"]=>
  string(3) "1e2"
}
