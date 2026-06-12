<?php
class Foo {
    public function __construct() {
        call_user_func(array($this, "hello"));
    }

    private function hello(): void {
        echo "hello";
    }
}
