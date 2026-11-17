--TEST--
app event dispatcher
--FILE--
<?php
class Dispatcher {
    private $listeners = [];
    public function on($name, callable $listener) {
        $this->listeners[$name][] = $listener;
    }
    public function dispatch($name, $event) {
        $results = [];
        foreach ($this->listeners[$name] as $listener) {
            try {
                $results[] = $listener($event);
            } catch (RuntimeException $error) {
                $results[] = $error->getMessage();
            }
        }
        return $results;
    }
}
$dispatcher = new Dispatcher;
$prefix = 'event:';
$seen = [];
$dispatcher->on('save', function ($event) use ($prefix, &$seen) {
    $seen[] = $event['id'];
    return $prefix . strtoupper($event['name']);
});
$dispatcher->on('save', function ($event) {
    if ($event['id'] < 2) {
        throw new RuntimeException('retry');
    }
    return 'ok';
});
var_dump($dispatcher->dispatch('save', ['id' => 1, 'name' => 'alice']));
var_dump($dispatcher->dispatch('save', ['id' => 2, 'name' => 'bob']));
var_dump($seen);
--EXPECT--
array(2) {
  [0]=>
  string(11) "event:ALICE"
  [1]=>
  string(5) "retry"
}
array(2) {
  [0]=>
  string(9) "event:BOB"
  [1]=>
  string(2) "ok"
}
array(2) {
  [0]=>
  int(1)
  [1]=>
  int(2)
}
