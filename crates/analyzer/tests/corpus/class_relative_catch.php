<?php
// Rule: class.notFound; class relative catch.
namespace App;
try {} catch (namespace\Missing $e) {} // error0
