<?php
// Rule: function.notFound; external_extension_good.
function run() {
    apcu_fetch('key', $found);
    if ($found) {
        return new Redis();
    }
    return new Memcached();
}
