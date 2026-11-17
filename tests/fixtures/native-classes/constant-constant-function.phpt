--TEST--
Native classes: constant-constant-function
--FILE--
<?php
var_dump(constant("PDO::FETCH_ASSOC"), constant("pdo::FETCH_NUM"));

--EXPECT--
int(2)
int(3)
