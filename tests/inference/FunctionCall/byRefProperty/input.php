<?php
class A {
    /** @var string */
    public $foo = "hello";
}

$a = new A();

function fooFoo(string &$v): void {}

fooFoo($a->foo);
