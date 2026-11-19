--TEST--
Taking a property reference suspends its getter at 50000 levels
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        &get {
            if (++self::$depth === 50000) {
                $terminal = 42;
                return $terminal;
            }
            $next = new Chain;
            $reference =& $next->value;
            return $reference;
        }
    }
}
$first = new Chain;
try {
    $reference =& $first->value;
    throw new Error("Resumed " . Chain::$depth . " getter frames with " . $reference);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Resumed 50000 getter frames with 42
