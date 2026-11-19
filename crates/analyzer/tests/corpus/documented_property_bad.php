<?php
// Rule: property.notFound; documented_property_bad.
/** @property-read int $count */
class Adapter {
    function run() { echo $this->counnt; }
}
