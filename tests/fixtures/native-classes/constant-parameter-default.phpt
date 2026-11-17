--TEST--
Native classes: constant-parameter-default
--FILE--
<?php
function value($mode=PDO::FETCH_ASSOC) {
 return $mode;
 }
 var_dump(value(), value(PDO::FETCH_NUM));

--EXPECT--
int(2)
int(3)
