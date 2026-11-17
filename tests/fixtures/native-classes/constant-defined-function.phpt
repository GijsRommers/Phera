--TEST--
Native classes: constant-defined-function
--FILE--
<?php
var_dump(defined("PDO::FETCH_ASSOC"),defined("PDO::NOPE"),defined("PDO::fetch_assoc"));

--EXPECT--
bool(true)
bool(false)
bool(false)
