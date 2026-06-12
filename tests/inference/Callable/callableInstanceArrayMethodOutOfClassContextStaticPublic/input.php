<?php
/**
 * @param callable $callable
 * @return void
 */
function run($callable) {
    call_user_func($callable);
}

class Foo {
    public function __construct() {
        run(array($this, "hello"));
    }

    public static function hello(): void {
        echo "hello";
    }
}
