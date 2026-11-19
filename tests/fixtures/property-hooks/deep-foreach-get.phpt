--TEST--
Foreach suspends the getter for the current property at 50000 levels
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            foreach (new Chain as $value) {
                return $value;
            }
        }
    }
}
$first = new Chain;
try {
    foreach ($first as $value) { echo $value; }
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 getter frames
