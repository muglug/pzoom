<?php
final class A {
    /**
     * @psalm-suppress UnusedMethod
     */
    public function foo() : void {}
}

new A();
