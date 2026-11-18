--TEST--
arrayaccess parameter contract
--FILE--
<?php

class Wrong implements ArrayAccess {public function offsetGet(string $offset):mixed{} public function offsetExists(mixed $offset):bool{return true;} public function offsetSet(mixed $offset,mixed $value):void{} public function offsetUnset(mixed $offset):void{}}
--EXPECTF--
Fatal error: Declaration of Wrong::offsetGet(string $offset): mixed must be compatible with ArrayAccess::offsetGet(mixed $offset): mixed in %s on line 3
