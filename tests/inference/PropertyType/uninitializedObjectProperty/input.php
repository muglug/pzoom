<?php
class Foo {
    /** @var int */
    public $bar = 5;
}
function takesInt(int $i) : void {}
class A {
    /** @var Foo */
    public $foo;

    public function __construct(Foo $foo) {
        takesInt($this->foo->bar);
        $this->foo = $foo;
    }
}
