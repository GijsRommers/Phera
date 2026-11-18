<?php
// Reproducer: the engine has no memory_limit, so this 3-line loop grows RSS
// without bound (observed > 2.4 GB in 4 s) and never terminates. Real PHP
// 8.5 with the default memory_limit=128M dies in ~0.06 s with
// "Allowed memory size of 134217728 bytes exhausted".
// Command: timeout 10 phera tests/security/no-memory-limit.php   (watch `ps -o rss=`)
$a = [];
while (true) {
    $a[] = str_repeat('x', 1024);
}
