<?php
// Rule: function.notFound; function bad import.
namespace App; use function Missing\go as g; g(); // error0
