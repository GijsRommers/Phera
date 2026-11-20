--TEST--
A returned array of handlers can be iterated by reference without mutating the source
--FILE--
<?php
function registered() {
    $args = [['methods' => 'GET']];
    foreach ($args as &$arg) {}
    return $args;
}
function routes($registered) {
    foreach ($registered as &$handler) {
        $methods = is_string($handler['methods']) ? explode(',', $handler['methods']) : $handler['methods'];
        $handler['methods'] = [];
        foreach ($methods as $method) {
            $handler['methods'][(string)$method] = true;
        }
    }
    return array_keys($registered[0]['methods']);
}
$original = registered();
var_dump(routes($original), routes($original));
--EXPECT--
array(1) {
  [0]=>
  string(3) "GET"
}
array(1) {
  [0]=>
  string(3) "GET"
}
