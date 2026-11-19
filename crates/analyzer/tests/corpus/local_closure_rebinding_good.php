<?php
// Rule: method.notFound; local_closure_rebinding_good.
// The callback runs with the receiver bound by the next statement.
class Receiver { function read() {} }
class Binder {
    function run(Receiver $receiver) {
        $callback = function () { $this->read(); };
        $callback = $callback->bindTo($receiver, $receiver);
        $callback();
    }
}
