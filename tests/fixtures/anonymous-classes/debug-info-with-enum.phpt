--TEST--
Anonymous class debug info displays the enum case returned by its hook
--FILE--
<?php
enum State { case Ready; }
$object = new class {
    public function __debugInfo(): array { return ['state' => State::Ready]; }
};
var_dump($object);
--EXPECTF--
object(class@anonymous)#%d (1) {
  ["state"]=>
  enum(State::Ready)
}
