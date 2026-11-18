--TEST--
traits: trait method wins over parent
--FILE--
<?php
class P { function m(){return "parent";} } trait T { function m(){return "trait";} } class C extends P {use T;} echo (new C)->m();
--EXPECT--
trait
