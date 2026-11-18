--TEST--
A thrown unserialize hook stops later hooks and propagates the same exception
--FILE--
<?php
class Wake {
    public $name;
    public function __construct($name) { $this->name = $name; }
    public function __wakeup() {
        echo "wake $this->name\n";
        if ($this->name === 'stop') throw new Exception('stop');
    }
}
$data = serialize([new Wake('first'), new Wake('stop'), new Wake('last')]);
try { unserialize($data); } catch (Exception $e) { echo "caught ", $e->getMessage(), "\n"; }
?>
--EXPECT--
wake first
wake stop
caught stop
