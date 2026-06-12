<?php
class Foo {
    /** @var string */
    public $foo = "";
}

/** @var mixed */
$a = (new Foo());

$a->foo = "hello";
