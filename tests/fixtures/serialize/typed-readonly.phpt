--TEST--
Unserialize can initialize readonly properties and uses strict property types
--FILE--
<?php
class Typed {
    private readonly int $one;
    public function number() { return $this->one; }
}
$valid = 'O:5:"Typed":1:{s:3:"one";i:15;}';
var_dump(unserialize($valid)->number());
try { unserialize('O:5:"Typed":1:{s:3:"one";s:2:"15";}'); }
catch (TypeError $e) { echo $e->getMessage(), "\n"; }
?>
--EXPECT--
int(15)
Cannot assign string to property Typed::$one of type int
