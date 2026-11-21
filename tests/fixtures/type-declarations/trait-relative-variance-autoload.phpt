--TEST--
Trait contracts with self and parent can autoload an overriding return type
--FILE--
<?php
class Base {}
trait RequiredMethods {
    abstract private function own(): self;
    abstract private function inherited(): parent;
}

spl_autoload_register(function (string $name): void {
    echo "autoload $name\n";
    if ($name === 'ForSelf') {
        class ForSelf extends Owner {}
    }
    if ($name === 'ForParent') {
        class ForParent extends Base {}
    }
});

class Owner extends Base {
    use RequiredMethods;

    private function own(): ForSelf { return new ForSelf; }
    private function inherited(): ForParent { return new ForParent; }
}

var_dump(new Owner instanceof Base);
?>
--EXPECT--
autoload ForSelf
autoload ForParent
bool(true)
