<?php
class CursorWrapper
{
    public $context;
    private int $cursor = 0;
    public function dir_opendir($path, $flags) { echo "open $path $flags\n"; return true; }
    public function dir_readdir() {
        echo "read $this->cursor\n";
        return ['first', 'second'][$this->cursor++] ?? false;
    }
    public function dir_rewinddir() { echo "rewind\n"; $this->cursor = 0; return true; }
    public function dir_closedir() { echo "close\n"; return true; }
    public function url_stat($path, $flags) {
        echo "stat $path $flags\n";
        return ['mode' => 0100644, 'size' => 7];
    }
}
stream_wrapper_register('cursor-wrapper', CursorWrapper::class);
$iterator = new DirectoryIterator('cursor-wrapper://directory');
echo $iterator->getFilename(), "\n";
foreach ($iterator as $entry) {
    echo $entry->getFilename(), "\n";
    var_dump($entry->isFile(), $entry->getSize());
}
unset($entry, $iterator);
echo "done\n";
