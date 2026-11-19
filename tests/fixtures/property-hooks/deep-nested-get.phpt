--TEST--
Hook frames resume a chained property read after deep calls
--FILE--
<?php
class Box {
    public function __construct(public Chain $item) {}
}
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                return 1;
            }
            return (new Box(new Chain))->item->value + 1;
        }
    }
}
try {
    throw new Error('Resumed ' . (new Chain)->value . ' getter frames');
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Resumed 50000 getter frames
