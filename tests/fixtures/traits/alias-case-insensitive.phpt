--TEST--
traits: alias case insensitive
--FILE--
<?php
trait T{function camelCase(){return 3;}} class C{use T {CAMELCASE as other;}} echo (new C)->other();
--EXPECT--
3
