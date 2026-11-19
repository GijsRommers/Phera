<?php
// Rule: method.notFound / property.notFound; documented_members_good.
class Source { function read() {} }
/**
 * @method string printLabel()
 * @property-read int $count
 * @mixin Source
 */
class Adapter {
    function __get($name) { return 7; }
    function run() {
        $this->read();
        $this->printLabel();
        echo $this->count;
    }
}
