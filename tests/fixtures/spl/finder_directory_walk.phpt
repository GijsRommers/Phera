--TEST--
Finder walks directory metadata through RecursiveIteratorIterator
--FILE--
<?php
$root = __DIR__ . '/finder-' . uniqid();
mkdir($root);
mkdir($root . '/lib');
file_put_contents($root . '/root.php', '<?php');
file_put_contents($root . '/lib/child.php', '<?php');
touch($root . '/root.php', 1700000001);
touch($root . '/lib/child.php', 1700000002);
touch($root . '/lib', 1700000003);
try {
    $source = new RecursiveDirectoryIterator($root, FilesystemIterator::SKIP_DOTS);
    $files = new RecursiveIteratorIterator($source, RecursiveIteratorIterator::SELF_FIRST);
    $lines = [];
    foreach ($files as $key => $file) {
        $path = $file->getPathname();
        $lines[] = substr($path, strlen($root) + 1)
            . ' ' . ($file->isDir() ? 'dir' : 'file')
            . ' ' . $file->getMTime()
            . ' ' . ($file->getRealPath() === realpath($path) ? 'resolved' : 'unresolved')
            . ' ' . ($key === $path ? 'key' : 'wrong-key');
    }
    sort($lines, SORT_STRING);
    foreach ($lines as $line) {
        echo $line, "\n";
    }
} finally {
    unlink($root . '/lib/child.php');
    unlink($root . '/root.php');
    rmdir($root . '/lib');
    rmdir($root);
}
--EXPECT--
lib dir 1700000003 resolved key
lib/child.php file 1700000002 resolved key
root.php file 1700000001 resolved key
