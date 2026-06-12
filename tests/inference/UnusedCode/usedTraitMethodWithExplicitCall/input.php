<?php
class A {
    public function foo(): void {
        echo "parent method";
    }
}

trait T {
    public function foo(): void {
        echo "trait method";
    }
}

final class B extends A {
    use T;
}

(new A)->foo();
(new B)->foo();
