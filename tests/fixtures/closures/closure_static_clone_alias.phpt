--TEST--
closure static clone alias
--FILE--
<?php
$f=function(){static $n=0;return ++$n;};$f();$g=$f->bindTo(null);var_dump($g(),$g(),$f());
--EXPECT--
int(2)
int(3)
int(2)
