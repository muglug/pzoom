<?php
namespace Foo;
class A {
    /**
     * @return string
     */
    public function foo() {
        return "hello";
    }
}

class B extends A {
    /**
     * @return never-returns
     */
    public function foo() {
        exit();
    }
}
