--TEST--
Legacy __sleep selects properties in the hook order and reports duplicate and missing names
--FILE--
<?php
class ParentData { private $secret = 2; }
class ChildData extends ParentData {
    protected $one = 1;
    private $two = 2;
    public int $empty;
    public function __sleep() { return ['two', 'one', 'one', 'empty', "\0ParentData\0secret", 'missing']; }
}
echo bin2hex(serialize(new ChildData)), "\n";
?>
--EXPECT--

Warning: serialize(): "one" is returned from __sleep() multiple times in /private/tmp/phera-serialize-fixture-sleep.php on line 9

Warning: serialize(): "missing" returned as member variable from __sleep() but does not exist in /private/tmp/phera-serialize-fixture-sleep.php on line 9
4f3a393a224368696c6444617461223a333a7b733a31343a22004368696c64446174610074776f223b693a323b733a363a22002a006f6e65223b693a313b733a31383a2200506172656e744461746100736563726574223b693a323b7d
