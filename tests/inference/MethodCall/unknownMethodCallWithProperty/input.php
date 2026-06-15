<?php
class A {
    private string $b = "c";

    public function passesByRef(object $a): void {
        $a->passedByRef($this->b);
    }
}
