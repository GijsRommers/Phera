--TEST--
Native classes: constant-class-case
--FILE--
<?php
var_dump(pdo::FETCH_NUM, \PDO::FETCH_ASSOC);

--EXPECT--
int(3)
int(2)
