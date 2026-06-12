<?php
class Foo {
    public function __construct() {
        call_user_func(array(Foo::class, "hello"));
    }

    protected function hello(): void {
        echo "hello";
    }
}
