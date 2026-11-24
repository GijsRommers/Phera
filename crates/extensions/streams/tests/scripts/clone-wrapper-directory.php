<?php
class CloneDirectoryWrapper
{
    public $context;
    private static int $next = 0;
    private int $instance;
    private int $index = 0;

    public function __construct()
    {
        $this->instance = ++self::$next;
    }

    public function dir_opendir($path, $options)
    {
        echo "$this->instance open $path $options\n";
        return true;
    }

    public function dir_readdir()
    {
        $entry = ['.', '..', 'alpha', 'beta'][$this->index++] ?? false;
        echo "$this->instance read ", json_encode($entry), "\n";
        return $entry;
    }

    public function dir_rewinddir()
    {
        echo "$this->instance rewind\n";
        $this->index = 0;
        return true;
    }

    public function dir_closedir()
    {
        echo "$this->instance close\n";
        return true;
    }
}

stream_wrapper_register('cloneddir', CloneDirectoryWrapper::class);
foreach ([DirectoryIterator::class, FilesystemIterator::class, RecursiveDirectoryIterator::class] as $class) {
    echo "$class\n";
    $directory = new $class('cloneddir://example');
    $directory->next();
    $copy = clone $directory;
    var_dump($directory->getFilename(), $copy->getFilename());
    $copy->next();
    $directory->rewind();
    var_dump($directory->getFilename(), $copy->getFilename());
    unset($directory, $copy);
}
