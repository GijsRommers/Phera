--TEST--
traits: parent method
--FILE--
<?php
class P{function p(){return "P";}} trait T{function m(){return parent::p();}} class C extends P{use T;} echo (new C)->m();
--EXPECT--
P
