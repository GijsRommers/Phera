--TEST--
traits: class method wins
--FILE--
<?php
trait T { function m(){return "trait";} } class C { use T; function m(){return "class";} } echo (new C)->m();
--EXPECT--
class
