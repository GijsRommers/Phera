--TEST--
var_export retains its place across deep getter calls
--FILE--
<?php
class ViewChain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) throw new Error('Reached 50000 native getter frames');
            var_export(new ViewChain);
            return 1;
        }
    }
}
try {
    var_export(new ViewChain);
} catch (Error $e) {
    echo ViewChain::$depth, ' ', $e->getMessage(), "\n";
}
--EXPECT--
50000 Reached 50000 native getter frames
