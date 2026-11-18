--TEST--
Child scope dispatches an inaccessible private ancestor through __call
--FILE--
<?php
class A { private function hidden() { echo "real"; } }
class B extends A { function __call($n, $a) { echo $n; } function run() { $this->hidden(); } }
(new B)->run();
--EXPECT--
hidden
