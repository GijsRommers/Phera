--TEST--
Native classes: constant-closure-default
--FILE--
<?php
$f=function($mode=PDO::FETCH_ASSOC) {
 return $mode;
 }
;
 var_dump($f());

--EXPECT--
int(2)
