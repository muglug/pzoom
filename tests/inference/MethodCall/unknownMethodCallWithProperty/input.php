<?php
class A {
    private string $b = "c";

    public function passesByRef(object $a): void {
        /** @psalm-suppress MixedMethodCall */
        $a->passedByRef($this->b);
    }
}
