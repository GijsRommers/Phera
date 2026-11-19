--TEST--
json_encode retains its place across deep getter calls
--FILE--
<?php
class ViewChain {
    public static int $depth = 0;
    public int $value {
        get {
            if (++self::$depth === 50000) throw new Error('Reached 50000 native getter frames');
            $next = new ViewChain;
            if (self::$depth % 4 !== 0) return $next->value;
            json_encode($next);
            return 1;
        }
    }
}
try {
    json_encode(new ViewChain);
} catch (Error $e) {
    echo ViewChain::$depth, ' ', $e->getMessage(), "\n";
}
--EXPECT--
50000 Reached 50000 native getter frames
