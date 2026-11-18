--TEST--
properties unset private
--FILE--
<?php
class Overloaded {
    private $hidden = 1;
    protected $protected = 2;
    public $public = 3;
    public int $typed;
    private $data = ['count' => 5, 'zero' => 0, 'null' => null];
    public function __get($name) { echo "get $name\n"; return $this->data[$name] ?? "[$name]"; }
    public function __set($name, $value) { echo "set $name\n"; $this->data[$name] = $value; }
    public function __isset($name) { echo "isset $name\n"; return array_key_exists($name, $this->data); }
    public function __unset($name) { echo "unset $name\n"; unset($this->data[$name]); }
    public function inside() { echo $this->hidden; }
}
$o = new Overloaded;

unset($o->hidden);
--EXPECT--
unset hidden
