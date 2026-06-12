<?php
class A {
    /** @return ?string */
    public function foo() {
        return rand(0, 1) ? "hello" : null;
    }
}

class B extends A {
    public function foo(): void {
        return;
    }
}

class C extends A {
    /** @return void */
    public function foo() {
        return;
    }
}

class D extends A {
    /** @return null */
    public function foo() {
        return null;
    }
}
