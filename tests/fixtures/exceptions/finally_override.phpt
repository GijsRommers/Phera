--TEST--
Exceptions: finally_override
--FILE--
<?php
function run() {try {throw new Error("x");} finally {return 8;}} echo run();
--EXPECTF--
8