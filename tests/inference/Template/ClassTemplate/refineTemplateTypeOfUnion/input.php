<?php
/** @psalm-template T as One|Two|Three */
class A {
    /** @param T $t */
    public function __construct(
        private object $t
    ) {}

    public function foo(): void {
        if ($this->t instanceof One || $this->t instanceof Two) {}
    }
}

final class One {}
final class Two {}
final class Three {}
