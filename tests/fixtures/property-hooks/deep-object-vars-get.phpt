--TEST--
get_object_vars suspends a getter at 50000 levels
--FILE--
<?php
class Chain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) {
                throw new Error('Reached ' . self::$depth . ' getter frames');
            }
            return get_object_vars(new Chain)['value'];
        }
    }
}
try {
    get_object_vars(new Chain);
} catch (Error $error) {
    echo $error->getMessage(), "\n";
}
--EXPECT--
Reached 50000 getter frames
