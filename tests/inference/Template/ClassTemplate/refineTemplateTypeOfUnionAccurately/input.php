<?php
/** @psalm-template T as One|Two|Three */
class A {
    /** @param T $t */
    public function __construct(
        private object $t
    ) {}

    /** @return int */
    public function foo() {
        if ($this->t instanceof One || $this->t instanceof Two) {
            return $this->t;
        }

        throw new \Exception();
    }
}

final class One {}
final class Two {}
final class Three {}
