--TEST--
Modern serialization hooks take priority and restore children before their parent
--FILE--
<?php
class Hook {
    public string $label;
    public ?Hook $child = null;
    public function __construct($label, $child = null) { $this->label = $label; $this->child = $child; }
    public function __serialize(): array { echo "save $this->label\n"; return [$this->label, $this->child, $this->child]; }
    public function __unserialize(array $fields): void {
        $this->label = $fields[0];
        $this->child = $fields[1];
        echo "restore $this->label ", $this->child?->label ?? '-', "\n";
    }
    public function __sleep() { echo "sleep\n"; return []; }
    public function __wakeup() { echo "wake\n"; }
}
$blob = serialize(new Hook('outer', new Hook('inner')));
echo $blob, "\n";
var_dump(unserialize($blob)->child->label);
?>
--EXPECT--
save outer
save inner
O:4:"Hook":3:{i:0;s:5:"outer";i:1;O:4:"Hook":3:{i:0;s:5:"inner";i:1;N;i:2;N;}i:2;r:3;}
restore inner -
restore outer inner
string(5) "inner"
