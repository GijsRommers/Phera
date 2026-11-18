<?php
// Rule: function.notFound; deliberate single error: function_namespace_relative.
namespace App; function f() { namespace\unavailable(); }
