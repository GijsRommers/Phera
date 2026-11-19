<?php
// Rule: variable.undefined; documented_closure_binding_good.
class Binder {
    public function read() {}
    /** @param-closure-this static $callback */
    public static function defer(Closure $callback) {}
}
Binder::defer(function () { $this->read(); });
