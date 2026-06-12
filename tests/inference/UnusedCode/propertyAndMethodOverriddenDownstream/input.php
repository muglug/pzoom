<?php
class A {
    /** @var string */
    public $foo = "hello";

    public function bar() : void {}
}

final class B extends A {
    /** @var string */
    public $foo = "goodbye";

    public function bar() : void {}
}

function foo(A $a) : void {
    echo $a->foo;
    $a->bar();
}

foo(new B());
