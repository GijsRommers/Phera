--TEST--
invoke reference
--FILE--
<?php
class Ref{public $x=2;function &__invoke(){return $this->x;}}$f=new Ref;$x=&$f();$x=9;var_dump($f->x);
--EXPECT--
int(9)
