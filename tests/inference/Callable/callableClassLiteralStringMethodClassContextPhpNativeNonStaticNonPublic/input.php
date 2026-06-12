<?php
class Foo {
    public function __construct() {
        call_user_func("Foo::hello");
    }

    protected function hello(): void {
        echo "hello";
    }
}
