--TEST--
Native classes: constant-direct
--FILE--
<?php
var_dump(PDO::FETCH_ASSOC, PDO::ATTR_ERRMODE, PDO::ERRMODE_EXCEPTION);

--EXPECT--
int(2)
int(3)
int(2)
