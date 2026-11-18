<?php
$d=new SQLite3(':memory:');
foreach(["","it's","a\0b","\xff","\\path"] as $v)var_dump(SQLite3::escapeString($v));
