--TEST--
bind detach unused this
--FILE--
<?php
class Box{function f(){return function(){return 3;};}}$f=(new Box)->f();$g=$f->bindTo(null);var_dump($g());
--EXPECT--
int(3)
