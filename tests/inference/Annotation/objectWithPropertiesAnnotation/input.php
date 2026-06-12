<?php
/** @param object{foo:string} $o */
function foo(object $o) : string {
    return $o->foo;
}

$s = new \stdClass();
$s->foo = "hello";
foo($s);

class A {
    /** @var string */
    public $foo = "hello";
}

foo(new A);
