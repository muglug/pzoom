<?php
class Foo {
    public function __construct() {
        call_user_func(array(__CLASS__, "hello"));
    }

    protected function hello(): void {
        echo "hello";
    }
}
