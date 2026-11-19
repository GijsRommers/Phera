<?php
// Rule: property.notFound; property_exists_good.
class Consumer {
    function field() { return property_exists($this, 'legacy') ? $this->legacy : 0; }
}
