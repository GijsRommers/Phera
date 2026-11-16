--TEST--
function_scope
--FILE--
<?php
function f(){$x=1;$r=include __DIR__.'/shared.inc';echo $x,$created,$r;}f();
--EXPECT--
inc3942