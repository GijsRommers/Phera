<?php
// Rule: function.notFound; deliberate single error: function_closure_body.
$callback = static function () { unavailable(); };
