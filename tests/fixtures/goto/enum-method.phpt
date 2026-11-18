--TEST--
An enum method resolves its own forward label
--FILE--
<?php
goto done;
echo "skipped file code\n";
done:

enum Route {
    case Ready;

    public function describe(): string {
        goto done;
        return "skipped method code";
        done:
        return $this->name;
    }
}
echo Route::Ready->describe(), "\n";
--EXPECT--
Ready
