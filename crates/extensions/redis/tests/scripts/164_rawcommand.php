<?php
require __DIR__."/../support.php";
check(fn()=>$r->rawCommand("SET",$p."k","v\0"));
check(fn()=>$r->rawCommand("GET",$p."k"));check(fn()=>$r->rawCommand("GET",$p."missing"));
check(fn()=>$r->rawCommand("PING"));
finish();
