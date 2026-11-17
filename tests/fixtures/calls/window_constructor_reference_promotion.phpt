--TEST--
Call window: constructor reference promotion
--FILE--
<?php
class A{function __construct(public &$x){$x++;}}$x=3;$a=new A($x);$a->x++;var_dump($x);
--EXPECT--
int(5)
