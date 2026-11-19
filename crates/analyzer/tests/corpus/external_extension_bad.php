<?php
// Rule: function.notFound; external_extension_bad.
function run() {
    if (extension_loaded('apcu')) {
        apcu_ftech('key');
    }
}
