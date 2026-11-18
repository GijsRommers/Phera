<?php
// Rule: function.notFound; deliberate single error: function_namespace_absolute.
namespace App; function f() { \unavailable(); }
