--TEST--
unserialize-failure-no-destructor
--FILE--
<?php
// unserialize() that fails: objects whose __wakeup threw or never ran get
// no destructor; objects without one, and those already woken, do.
class Plain { public $n; function __destruct() { echo "destruct plain {$this->n}\n"; } }
class Woke { public $n; function __wakeup() { echo "wakeup {$this->n}\n"; } function __destruct() { echo "destruct woke {$this->n}\n"; } }
class Bad { public $n; function __wakeup() { throw new Exception("bad"); } function __destruct() { echo "destruct bad\n"; } }
class Uns { public $n; function __unserialize(array $d) { echo "unserialize\n"; $this->n = $d['n']; } function __destruct() { echo "destruct uns {$this->n}\n"; } }
echo "-- plain object, then a parse error\n";
var_dump(unserialize('a:2:{i:0;O:5:"Plain":1:{s:1:"n";s:1:"a";}i:1;BROKEN'));
echo "-- woken object, then a throwing wakeup\n";
try { unserialize('a:2:{i:0;O:4:"Woke":1:{s:1:"n";s:1:"b";}i:1;O:3:"Bad":0:{}}'); } catch (Exception $e) { echo "caught\n"; }
echo "-- throwing wakeup first, woken object after\n";
try { unserialize('a:2:{i:0;O:3:"Bad":0:{}i:1;O:4:"Woke":1:{s:1:"n";s:1:"c";}}'); } catch (Exception $e) { echo "caught\n"; }
echo "-- __unserialize object, then a parse error\n";
var_dump(unserialize('a:2:{i:0;O:3:"Uns":1:{s:1:"n";s:1:"d";}i:1;BROKEN'));
echo "-- success\n";
$ok = unserialize('O:4:"Woke":1:{s:1:"n";s:1:"e";}'); unset($ok);
echo "end\n";
--EXPECTF--
-- plain object, then a parse error

Warning: unserialize(): Error at offset 45 of 51 bytes in %s on line 9
destruct plain a
bool(false)
-- woken object, then a throwing wakeup
wakeup b
destruct woke b
caught
-- throwing wakeup first, woken object after
caught
-- __unserialize object, then a parse error

Warning: unserialize(): Error at offset 43 of 49 bytes in %s on line 15
unserialize
destruct uns d
bool(false)
-- success
wakeup e
destruct woke e
end
