--TEST--
Serialization preserves PHP's precision-hook behavior
--FILE--
<?php
class PrecisionSwitch {
    public function __serialize(): array {
        ini_set('serialize_precision', '3');
        return [1.23456];
    }
}
echo serialize([1.23456, new PrecisionSwitch, 1.23456]), "\n";
--EXPECT--
a:3:{i:0;d:1.23456;i:1;O:15:"PrecisionSwitch":1:{i:0;d:1.23;}i:2;d:1.23;}
