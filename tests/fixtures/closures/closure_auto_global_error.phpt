--TEST--
closure_auto_global_error
--FILE--
<?php
$f=function()use($GLOBALS){};
--EXPECTF--
Fatal error: Cannot use auto-global as lexical variable in %s on line 2
