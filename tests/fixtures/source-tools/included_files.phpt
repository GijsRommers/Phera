--TEST--
PHP source tools match included_files
--FILE--
<?php
$dir = tempnam(sys_get_temp_dir(), 'phera-included-');
unlink($dir);
mkdir($dir);
$start = $dir . '/first.php';
$end = $dir . '/second.php';
file_put_contents($start, '<?php require __DIR__ . "/second.php";');
file_put_contents($end, '<?php echo "second\n";');
echo count(get_included_files()), "\n";
require $start;
include_once $start;
require $end;
eval('echo count(get_required_files()), "\n";');
foreach (get_included_files() as $i => $file) {
    echo $i, ' ', $file === __FILE__ ? '<main>' : basename($file), ' ', $file === realpath($file) ? 'absolute' : 'relative', "\n";
}
var_dump(get_required_files() === get_included_files());
unlink($start);
unlink($end);
rmdir($dir);

--EXPECT--
1
second
second
3
0 <main> absolute
1 first.php absolute
2 second.php absolute
bool(true)
