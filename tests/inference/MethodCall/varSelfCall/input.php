<?php
class Foo {
    /** @var self */
    public static $current;
    public function bar() : void {}
}

Foo::$current->bar();
