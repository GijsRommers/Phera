<?php
// Rule: class.notFound; class local catch.
namespace App;
try {} catch (Missing $e) {} // error0
