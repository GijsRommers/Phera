<?php
// Rule: class.notFound; class absolute catch.
try {} catch (\Missing $e) {} // error0
