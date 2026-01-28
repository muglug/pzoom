<?php
namespace Foo;

/** @psalm-no-seal-properties */
class A {
    /** @var string */
    protected $foo = C::DOPE;

    /** @return string */
    public function __get(string $s) {
        return "foo";
    }
}

class B extends A {
    /** @return void */
    public function foo() {
        echo (new C)->bar;
    }
}

class C extends B {
    const DOPE = "dope";
}
