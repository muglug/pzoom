<?php
class Foo {
    public array $foo = [];
};


/** @psalm-assert array{a:1} $o->foo */
function change(Foo $o): void
{
    $o->foo = ["a" => 1];
}
$o = new Foo;
change($o);
