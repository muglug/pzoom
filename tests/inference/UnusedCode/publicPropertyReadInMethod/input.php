<?php
final class A {
    public string $a = "hello";
}

final class B {
    public function foo(A $a): void {
        if ($a->a === "goodbye") {}
    }
}

(new B)->foo(new A());
