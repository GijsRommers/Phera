--TEST--
Native classes: constant-global
--FILE--
<?php
const MODE=PDO::FETCH_NUM;
 var_dump(MODE);

--EXPECT--
int(3)
