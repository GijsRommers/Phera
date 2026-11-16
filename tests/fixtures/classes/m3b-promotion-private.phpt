--TEST--
Class members: promotion-private
--FILE--
<?php class Foo {function __construct(private int $x=7){} function read(){return $this->x;}} var_dump((new Foo)->read());
--EXPECT--
int(7)
