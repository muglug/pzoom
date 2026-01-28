<?php
class A {
    public function b() {
        /**
         * @psalm-suppress UndefinedClass
         */
        new B();
        new C();
    }
}
