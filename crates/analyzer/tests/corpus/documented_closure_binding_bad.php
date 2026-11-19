<?php
// Rule: variable.undefined; documented_closure_binding_bad.
class Binder {
    public function read() {}
    public static function defer(Closure $callback) {}
}
Binder::defer(function () { $this->read(); });
