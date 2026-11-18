<?php
$directory = "native\xFF";
$path = "file\xA1.php";
set_include_path($directory);
var_dump(bin2hex(get_include_path()));
if (!mkdir($directory)) {
    // Some filesystems reject invalid UTF-8. Compare that failure too,
    // then exercise the same native operations on names the filesystem allows.
    var_dump(file_exists($directory), stream_resolve_include_path($path));
    $directory = "native-\xC3\xA9";
    $path = "file-\xC2\xA1.php";
    mkdir($directory);
}
file_put_contents("$directory/$path", '<?php return "raw path";');
set_include_path($directory);
var_dump(bin2hex(get_include_path()));
var_dump(bin2hex(basename(stream_resolve_include_path($path))));
var_dump(file_get_contents($path, true));
var_dump(require $path);
chdir($directory);
var_dump(file_get_contents($path));
