<?php
final class A {
    /**
     * @psalm-suppress UnusedMethod
     * @return void
     */
    private function foo() {}
}

new A();
