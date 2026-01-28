<?php
class Foo {
    /** @var self */
    public static $current;
}

$a = Foo::$current;
