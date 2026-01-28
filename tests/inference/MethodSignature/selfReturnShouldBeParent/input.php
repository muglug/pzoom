<?php
class A {
    /** @return self */
    public function foo() {
        return new A();
    }
}

class B extends A {
    public function foo() {
        return new A();
    }
}
