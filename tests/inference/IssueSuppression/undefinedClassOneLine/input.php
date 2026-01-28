<?php
class A {
    public function b(): void {
        /**
         * @psalm-suppress UndefinedClass
         */
        new B();
    }
}
