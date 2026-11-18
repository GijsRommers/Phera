<?php
require __DIR__."/../support.php";
$sha=$r->script("LOAD","return ARGV[1]");
check(fn()=>$r->evalSha($sha,["abc"],0));check(fn()=>$r->script("EXISTS",$sha));
finish();
