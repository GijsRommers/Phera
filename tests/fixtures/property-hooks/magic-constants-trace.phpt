--TEST--
A getter has hook magic constants and a PHP method-shaped trace frame
--FILE--
<?php
namespace App;
class C {
    public int $x {
        get {
            echo __PROPERTY__, "\n", __FUNCTION__, "\n", __METHOD__, "\n";
            throw new \Exception('failure');
        }
    }
}
try { $unused = (new C)->x; } catch (\Exception $e) {
    $frame = $e->getTrace()[0];
    echo $frame['class'], $frame['type'], $frame['function'], "()\n";
}
--EXPECT--
x
$x::get
App\C::$x::get
App\C->$x::get()
