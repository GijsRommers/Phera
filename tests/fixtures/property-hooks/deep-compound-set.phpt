--TEST--
Compound writes suspend the setter after the getter at 50000 levels
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        get => 1;
        set {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' setter frames');
            }
            $next = new Chain;
            $next->value += $value;
        }
    }
}
$first = new Chain;
try {
    $first->value = 1;
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 setter frames
