--TEST--
Read bytes written by PHP including NUL-mangled property names
--FILE--
<?php
class Secret {
    private string $pass;
    protected int $count;
    public function values() { return [$this->pass, $this->count]; }
}
$bytes = "O:6:\"Secret\":2:{s:12:\"\0Secret\0pass\";s:3:\"a\0b\";s:8:\"\0*\0count\";i:7;}";
$r = unserialize($bytes);
var_dump($r->values()[1], bin2hex($r->values()[0]), serialize($r) === $bytes);
?>
--EXPECT--
int(7)
string(6) "610062"
bool(true)
