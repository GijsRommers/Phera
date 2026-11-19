<?php
// Rule: property.notFound; property_exists_bad.
class Consumer {
    function field() { return property_exists($this, 'older') ? $this->legacy : 0; }
}
