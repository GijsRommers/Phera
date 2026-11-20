--TEST--
temporary-file-removed
--FILE--
<?php
class TemporaryFile {
    public string $path;
    public function __construct() { $this->path = tempnam(sys_get_temp_dir(), "phera"); file_put_contents($this->path, "data"); }
    public function __destruct() { unlink($this->path); }
}
function useFile(): string { $file = new TemporaryFile; echo "exists: ", var_export(file_exists($file->path), true), "\n"; return $file->path; }
$path = useFile();
echo "after: ", var_export(file_exists($path), true), "\n";
--EXPECT--
exists: true
after: false
