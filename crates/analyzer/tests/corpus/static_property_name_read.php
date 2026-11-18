<?php
// Rule: control flow / variable.undefined / return; static_property_name_read.
class C {}
echo C::${$missing}; // error0
