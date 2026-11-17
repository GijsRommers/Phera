--TEST--
arrow closure uses
--FILE--
<?php
$x=8;$f=fn()=>function()use($x){return $x;};$g=$f();var_dump($g());
--EXPECT--
int(8)
