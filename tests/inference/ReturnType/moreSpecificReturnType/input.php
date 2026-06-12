<?php
class A {}
class B extends A {}
interface I {
    /** @return B[] */
    public function foo();
}
class D implements I {
    /** @return A[] */
    public function foo() {
        return [new A, new A];
    }
}
