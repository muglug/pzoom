<?php
class A {
    /** @var string */
    public $foo = "hello";
}

final class B extends A {
    /** @var string */
    public $foo = "goodbye";
}

new B();
