--TEST--
Exceptions: throw_coalesce_skip
--FILE--
<?php
$value=7; echo $value ?? throw new Exception("missing");
--EXPECTF--
7