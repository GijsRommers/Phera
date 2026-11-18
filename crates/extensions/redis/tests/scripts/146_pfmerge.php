<?php
require __DIR__."/../support.php";
check(fn()=>$r->pfMerge($p."b",[$p."a"]));$r->pfAdd($p."a",["a","b"]);
check(fn()=>$r->pfMerge($p."b",[$p."a"]));check(fn()=>$r->pfCount($p."b"));
finish();
