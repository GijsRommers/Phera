<?php
require __DIR__."/../support.php";
foreach (["getHost","getPort","getDBNum","getTimeout","getPersistentID","getAuth",
"isConnected","ping","getReadTimeout","getLastError","clearLastError"] as $method) {
check(fn()=>$r->$method());}
check(fn()=>$r->echo("a\0b"));check(fn()=>$r->ping("hello"));
finish();
