<?php
trait T {
    /** @return void */
    abstract public function foo();
}

abstract class A {
    use T;

    /** @return void */
    public function bar() {
        $this->foo();
    }
}
