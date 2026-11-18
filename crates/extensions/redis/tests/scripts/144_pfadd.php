<?php
require __DIR__."/../support.php";
check(fn()=>$r->pfAdd($p."a",["a","b","a"]));$r->pfAdd($p."a",["a","b"]);
check(fn()=>$r->pfAdd($p."a",["a","b","a"]));check(fn()=>$r->pfCount($p."b"));
finish();
