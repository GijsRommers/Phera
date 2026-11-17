--TEST--
Native classes: constant-arrow-default
--FILE--
<?php
$f=fn($mode=PDO::FETCH_NUM)=>$mode;
 var_dump($f());

--EXPECT--
int(3)
