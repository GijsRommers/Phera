<?php
require __DIR__ . '/../support.php';
// The message of a refused connection is the operating system's text alone.
check(fn()=>(new Redis())->connect('127.0.0.1', 1, 0.5));
finish();
