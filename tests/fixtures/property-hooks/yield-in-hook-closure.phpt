--TEST--
A closure inside a hook owns its yield independently of the hook
--FILE--
<?php
class Source {
    private string $prefix = 'from closure';
    public Closure $callback {
        get {
            echo "getter\n";
            return function (): Generator {
                yield $this->prefix;
            };
        }
    }
}
$callback = (new Source())->callback;
echo get_class($callback), "\n";
var_dump(iterator_to_array($callback()));
--EXPECT--
getter
Closure
array(1) {
  [0]=>
  string(12) "from closure"
}
