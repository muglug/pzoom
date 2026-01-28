<?php
abstract class A {
    /** @return void */
    abstract public function foo();
}

abstract class B extends A {
    /** @return void */
    public function bar() {
        $this->foo();
    }
}
