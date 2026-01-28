<?php
/** @psalm-no-seal-methods */
class B {
    public function __call(string $methodName, array $args) : string {
        return __METHOD__;
    }
}
/** @psalm-no-seal-methods */
class A {
    public function __call(string $methodName, array $args) : B {
        return new B;
    }
}
$a = (new A)->zugzug();
$b = (new A)->bar()->baz();
