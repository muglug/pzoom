<?php
namespace Foo;

class A {
    public function getThis() : static {
        return $this;
    }
}

class B extends A {
    public function foo() : void {}
}

(new B)->getThis()->foo();
