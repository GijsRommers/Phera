<?php
// Rule: method.notFound; local_closure_rebinding_bad.
class Binder {
    function run() {
        $callback = function () { $this->read(); };
        $callback();
    }
}
