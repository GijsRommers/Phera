--TEST--
Include from eval
--FILE--
<?php
var_dump(eval('return require __DIR__ . "/nested-eval.inc";'));
--EXPECT--
int(19)
