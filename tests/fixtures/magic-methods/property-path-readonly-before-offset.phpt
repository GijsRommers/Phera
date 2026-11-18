--TEST--
Readonly array mutation fails before PHP converts the offset
--FILE--
<?php
class Box {
    public readonly array $data;
    public function __construct() { $this->data = []; }
}
$b = new Box;
try { $b->data[1.5]++; } catch (Error $e) { echo $e->getMessage(), "\n"; }
try { unset($b->data[1.5]); } catch (Error $e) { echo $e->getMessage(), "\n"; }
--EXPECT--
Cannot indirectly modify readonly property Box::$data
Cannot indirectly modify readonly property Box::$data
