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
        run("Foo::hello");
    }

    protected function hello(): void {
        echo "hello";
    }
}
