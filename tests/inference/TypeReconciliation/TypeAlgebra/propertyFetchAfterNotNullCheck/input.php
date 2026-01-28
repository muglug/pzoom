<?php
class A {
    /** @var ?string */
    public $foo;
}

$a = new A;

if ($a->foo === null) {
    $a->foo = "hello";
    exit;
}

if ($a->foo === "somestring") {}