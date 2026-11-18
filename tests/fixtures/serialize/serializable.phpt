--TEST--
Serializable retains PHP C format and takes priority over __sleep
--FILE--
<?php
class Legacy implements Serializable {
    public function serialize(): string { return "a\0b"; }
    public function unserialize(string $data): void { echo bin2hex($data), "\n"; }
    public function __sleep() { echo "sleep\n"; return []; }
}
$blob = serialize(new Legacy);
echo bin2hex($blob), "\n";
var_dump(get_class(unserialize($blob)));
?>
--EXPECT--

Deprecated: Legacy implements the Serializable interface, which is deprecated. Implement __serialize() and __unserialize() instead (or in addition, if support for old PHP versions is necessary) in /private/tmp/phera-serialize-fixture-serializable.php on line 2
433a363a224c6567616379223a333a7b6100627d
610062
string(6) "Legacy"
