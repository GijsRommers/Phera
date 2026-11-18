<?php
require __DIR__."/../support.php";
check(fn()=>$r->eval('return {KEYS[1],ARGV[1],false}',[$p."k","a\0"],1));
check(fn()=>$r->eval('return 42'));check(fn()=>$r->eval('return redis.call("GET",KEYS[1])',[$p."missing"],1));
finish();
